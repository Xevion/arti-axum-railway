# arti-axum-railway

A demo project for running an Axum server behind an Arti onion service. Check it out [on Railway](https://arti-railway-production.up.railway.app).

- A single Dockerfile well-suited for deployment to Railway
- Proxies connections from the Tor network as an Onion service
- Also listens on a public port for direct or exit node connections

This demo is far more complex than what you're probably looking for, so I'm planning to create a barebones version later on.

## Resources

Unfortunately, the [main documentation for Arti](https://tpo.pages.torproject.net/core/arti/) is really quite lacking; their primary documentation includes literally nothing about running an onion service.

- [Default Arti Configuration](https://gitlab.torproject.org/tpo/core/arti/-/blob/main/crates/arti/src/arti-example-config.toml) - Really verbose, but it seems to include everything.
- [Arti CLI Reference](https://tpo.pages.torproject.net/core/doc/rust/arti/index.html#configuration) - Also includes some of the build features, very useful.
- [Arti Debian Dockerfile](https://gitlab.torproject.org/tpo/onion-services/onimages/-/blob/main/arti/debian/Dockerfile) ([Config](https://gitlab.torproject.org/tpo/onion-services/onimages/-/blob/main/arti/debian/onionservice.toml)) - If you're using Railway or wanting to create your own Docker image, follow this. [Alpine version](https://gitlab.torproject.org/tpo/onion-services/onimages/-/blob/main/arti/alpine/Dockerfile).
