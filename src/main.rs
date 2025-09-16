use std::env::{self, VarError};

use axum::{response::Html, routing::get, Router};
use tokio::net::TcpListener;

#[derive(Debug)]
enum Error {
    /// Error occuring during startup
    Startup(String),
    /// Error occurred after starting
    Runtime(String),
}

async fn onion_handler() -> Html<&'static str> {
    Html("<h1>Hello!</h1><p>You are connected via the Tor network (onion service).</p>")
}

async fn public_handler() -> Html<&'static str> {
    Html("<h1>Hello!</h1><p>You are connected via the public endpoint. If you reached this through the Tor network, your connection is indirect; otherwise, you're connected directly.</p>")
}

async fn run() -> Result<(), Error> {
    let onion_app = Router::new().route("/", get(onion_handler));
    let public_app = Router::new().route("/", get(public_handler));

    const DEFAULT_ONION_PORT: u16 = 3000;
    let onion_listener = TcpListener::bind(format!("0.0.0.0:{}", DEFAULT_ONION_PORT))
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
            Ok(port) => Ok(port),
            Err(parse_err) => Err(Error::Startup(format!(
                "Unable to parse PORT as u16: {parse_err:?}",
            ))),
        },
        Err(VarError::NotUnicode(unicode_err)) => Err(Error::Startup(format!(
            "PORT is not a valid unicode string: {unicode_err:?}",
        ))),
    }?;

    let public_listener = TcpListener::bind(format!("0.0.0.0:{}", public_port))
        .await
        .map_err(|e| Error::Startup(format!("Unable to bind public listener: {e:?}")))?;

    println!(
        "public endpoint listening on {address}",
        address = public_listener
            .local_addr()
            .map_err(|e| Error::Startup(format!("Unable to get local address: {e:?}")))?
    );

    let (onion_res, public_res) = tokio::join!(
        axum::serve(onion_listener, onion_app),
        axum::serve(public_listener, public_app)
    );

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

    // Ok will never be returned (until we add a SIGTERM handler for graceful shutdown)
    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("error: {e:?}");
    }
}
