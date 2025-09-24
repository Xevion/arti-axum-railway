use std::env::{self, VarError};
use std::sync::Arc;

use axum::{extract::State, response::Html, routing::get, Router};
use parking_lot::RwLock;
use regex::Regex;
use tokio::net::TcpListener;
use tokio::process::Command;
use tokio::signal;
use tokio::sync::broadcast;
use tokio::time::{sleep, Duration, Instant};

#[derive(Debug)]
enum Error {
    /// Error occuring during startup
    Startup(String),
    /// Error occurred after starting
    Runtime(String),
}

fn install_signal_forwarders(tx: broadcast::Sender<()>) {
    let tx1 = tx.clone();
    let tx2 = tx;

    tokio::spawn(async move {
        let ctrl_c = async {
            signal::ctrl_c()
                .await
                .expect("failed to install Ctrl+C handler");
        };

        #[cfg(unix)]
        let terminate = async {
            signal::unix::signal(signal::unix::SignalKind::terminate())
                .expect("failed to install signal handler")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => {
                println!("Received Ctrl+C, shutting down gracefully...");
                let _ = tx1.send(());
            },
            _ = terminate => {
                println!("Received SIGTERM, shutting down gracefully...");
                let _ = tx2.send(());
            },
        }
    });
}

/// Maximum number of times to relaunch the arti process before exiting the server.
const ARTI_MAX_RELAUNCHES: usize = 5;
/// Delay between arti relaunch attempts.
const ARTI_RESTART_BACKOFF_SECS: u64 = 3;

async fn supervise_arti(
    mut shutdown: broadcast::Receiver<()>,
    shutdown_tx: broadcast::Sender<()>,
) -> Result<(), ()> {
    let mut attempts: usize = 0;

    loop {
        if attempts >= ARTI_MAX_RELAUNCHES {
            eprintln!(
                "arti restart limit exceeded (>{}), requesting shutdown",
                ARTI_MAX_RELAUNCHES
            );
            let _ = shutdown_tx.send(());
            return Err(());
        }

        if attempts > 0 {
            println!(
                "restarting arti (attempt {} of {})",
                attempts + 1,
                ARTI_MAX_RELAUNCHES
            );
        }

        attempts += 1;

        let mut child = match Command::new("./arti")
            .arg("proxy")
            .arg("-c")
            .arg("/etc/arti/onionservice.toml")
            .kill_on_drop(true)
            .spawn()
        {
            Ok(child) => child,
            Err(err) => {
                eprintln!("failed to spawn arti: {:?}", err);
                sleep(Duration::from_secs(ARTI_RESTART_BACKOFF_SECS)).await;
                continue;
            }
        };

        tokio::select! {
            status = child.wait() => {
                match status {
                    Ok(status) => {
                        if status.success() {
                            eprintln!("arti exited successfully (unexpected), will relaunch after backoff");
                        } else {
                            eprintln!("arti exited with status {:?}", status.code());
                        }
                    }
                    Err(err) => {
                        eprintln!("failed to wait on arti: {:?}", err);
                    }
                }
                sleep(Duration::from_secs(ARTI_RESTART_BACKOFF_SECS)).await;
                // loop to relaunch
            }
            _ = shutdown.recv() => {
                // Received shutdown signal; terminate child and exit
                let _ = child.start_kill();
                let _ = child.wait().await;
                return Ok(());
            }
        }
    }
}

#[derive(Clone)]
struct AppState {
    onion_address: Arc<RwLock<Option<String>>>,
}

async fn onion_handler(State(state): State<Arc<AppState>>) -> Html<String> {
    let maybe_addr = state.onion_address.read().clone();
    match maybe_addr {
        Some(addr) => Html(format!(
            "<h1>Hello!</h1><p>You are connected via the Tor network (onion service).</p><p>Onion address: <a href=\"http://{addr}\" rel=\"noopener noreferrer\">{addr}</a></p>"
        )),
        None => Html("<h1>Hello!</h1><p>You are connected via the Tor network (onion service).</p><p>Discovering onion address...</p>".to_string()),
    }
}

async fn public_handler(State(state): State<Arc<AppState>>) -> Html<String> {
    let maybe_addr = state.onion_address.read().clone();
    match maybe_addr {
        Some(addr) => Html(format!("<h1>Hello!</h1><p>You are connected via the public endpoint. If you reached this through the Tor network, your connection is indirect; otherwise, you're connected directly.</p><p>Tor onion service: <a href=\"http://{addr}\" rel=\"noopener noreferrer\">{addr}</a></p>")),
        None => Html("<h1>Hello!</h1><p>You are connected via the public endpoint. If you reached this through the Tor network, your connection is indirect; otherwise, you're connected directly.</p><p>Onion address is not available yet.</p>".to_string()),
    }
}

async fn run() -> Result<(), Error> {
    let state = Arc::new(AppState {
        onion_address: Arc::new(RwLock::new(None)),
    });

    let onion_app = Router::new()
        .route("/", get(onion_handler))
        .with_state(state.clone());
    let public_app = Router::new()
        .route("/", get(public_handler))
        .with_state(state.clone());

    const DEFAULT_ONION_PORT: u16 = 3000;

    // Bind to 127.0.0.1 to prevent external non-proxied access
    let onion_listener = TcpListener::bind(format!("127.0.0.1:{}", DEFAULT_ONION_PORT))
        .await
        .map_err(|e| Error::Startup(format!("Unable to bind onion listener: {e:?}")))?;
    println!(
        "onion endpoint listening on {}",
        onion_listener
            .local_addr()
            .map_err(|e| Error::Startup(format!("Unable to get local address: {e:?}")))?
    );

    // Acquire the public endpoint's port from the environment
    const DEFAULT_PORT: u16 = 8080;
    let public_port: u16 = match env::var("PORT") {
        Ok(string) if string.trim().is_empty() => Ok(DEFAULT_PORT),
        Err(VarError::NotPresent) => Ok(DEFAULT_PORT),
        Ok(port) => match port.parse::<u16>() {
            Ok(port) => {
                println!("Using PORT from environment: {}", port);
                Ok(port)
            }
            Err(parse_err) => Err(Error::Startup(format!(
                "Unable to parse PORT as u16: {parse_err:?}",
            ))),
        },
        Err(VarError::NotUnicode(unicode_err)) => Err(Error::Startup(format!(
            "PORT is not a valid unicode string: {unicode_err:?}",
        ))),
    }?;

    // Bind to 0.0.0.0 to allow external access
    let public_listener = TcpListener::bind(format!("0.0.0.0:{}", public_port))
        .await
        .map_err(|e| Error::Startup(format!("Unable to bind public listener: {e:?}")))?;

    println!(
        "public endpoint listening on {address}",
        address = public_listener
            .local_addr()
            .map_err(|e| Error::Startup(format!("Unable to get local address: {e:?}")))?
    );

    // Fire-and-forget task to discover the onion address from arti.
    {
        let state_for_task = state.clone();
        tokio::spawn(async move {
            // Delay 2 seconds after startup
            sleep(Duration::from_secs(2)).await;
            let deadline = Instant::now() + Duration::from_secs(30);
            let re = Regex::new(r"^[a-z2-7]{56}\.onion$").expect("valid regex");
            loop {
                let output = Command::new("./arti")
                    .arg("-c")
                    .arg("/etc/arti/onionservice.toml")
                    .arg("hss")
                    .arg("--nickname")
                    .arg("demo")
                    .arg("onion-address")
                    .output()
                    .await;

                if let Ok(output) = output {
                    if output.status.success() {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        if let Some(found) = stdout
                            .lines()
                            .map(|s| s.trim())
                            .find(|line| re.is_match(line))
                        {
                            {
                                let mut lock = state_for_task.onion_address.write();
                                *lock = Some(found.to_string());
                            }
                            println!("Discovered onion address: {}", found);
                            break;
                        }
                    }
                }

                if Instant::now() >= deadline {
                    println!("Failed to acquire onion address within timeout");
                    break;
                }

                sleep(Duration::from_secs(5)).await;
            }
        });
    }

    // Create shutdown channel and install signal forwarders
    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
    install_signal_forwarders(shutdown_tx.clone());

    // Clone the receiver for servers and arti supervisor
    let mut onion_shutdown = shutdown_rx.resubscribe();
    let mut public_shutdown = shutdown_rx.resubscribe();
    let arti_shutdown = shutdown_rx.resubscribe();

    // Start arti supervisor
    let arti_handle = tokio::spawn(supervise_arti(arti_shutdown, shutdown_tx.clone()));

    // Start both servers with graceful shutdown
    let onion_server = axum::serve(onion_listener, onion_app).with_graceful_shutdown(async move {
        let _ = onion_shutdown.recv().await;
    });

    let public_server =
        axum::serve(public_listener, public_app).with_graceful_shutdown(async move {
            let _ = public_shutdown.recv().await;
        });

    // Run both servers concurrently
    let (onion_res, public_res) = tokio::join!(onion_server, public_server);

    if let Err(e) = onion_res {
        return Err(Error::Runtime(format!(
            "onion endpoint service error: {e:?}"
        )));
    }

    if let Err(e) = public_res {
        return Err(Error::Runtime(format!(
            "public endpoint service error: {e:?}"
        )));
    }

    // Wait for arti supervisor to finish
    let arti_result = arti_handle.await;

    match arti_result {
        Ok(Ok(())) => {
            println!("Servers shut down gracefully");
            Ok(())
        }
        Ok(Err(())) => Err(Error::Runtime("arti restart limit exceeded".to_string())),
        Err(join_err) => Err(Error::Runtime(format!(
            "arti supervisor task failed to join: {join_err:?}"
        ))),
    }
}

#[tokio::main]
async fn main() {
    match run().await {
        Ok(()) => {}
        Err(e) => {
            eprintln!("error: {e:?}");
            std::process::exit(1);
        }
    }
}
