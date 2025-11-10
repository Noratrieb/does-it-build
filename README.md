# does it build?

A webapp that checks which Rust targets build at any nightly.

It does this by executing `cargo build --release -Zbuild-std=core` for every target and every nightly and displaying the result.

There's a background job that continously builds every target for every target that it hasn't built yet.
It does this in parallel, using half of the available threads (or `DOES_IT_BUILD_PARALLEL_JOBS`).


## Configuration

- `DB_PATH`: Path to SQlite DB to store the results
- `DOES_IT_BUILD_PARALLEL_JOBS`: Parallel build jobs, defaults to cores/2.
- `GITHUB_SEND_PINGS`: If this is set, actual pings will be sent for notification issues
- `GITHUB_OWNER`: The owner of the notification repo
- `GITHUB_REPO`: The repo name of the notification repo
- `GITHUB_APP_ID`: The app ID of the notification GitHub app
- `GITHUB_APP_PRIVATE_KEY`: The RSA private key for the notification GitHub app

Build configuration: `DOES_IT_BUILD_OVERRIDE_VERSION` to override the git commit.

## Deployment

deployed at <https://does-it-build.noratrieb.dev/>

## Notification

does-it-build supports sending target maintainer notifications on breakage.

It does this by creating an issue <https://github.com/Noratrieb/does-it-build-notifications> that pings the registered maintainers.
There is an array in the source code (linked to on the website target page) where people can add or remove themselves.

## License
Licensed under either of [Apache License, Version 2.0](./LICENSE-APACHE) or [MIT license](./LICENSE-MIT) at your option. 
