# does it build?

A webapp that checks which Rust targets build at any nightly.

It does this by executing `cargo build --release -Zbuild-std=core` for every target and every nightly and displaying the result.

There's a background job that continously builds every target for every target that it hasn't built yet.
It does this in parallel, using half of the available threads (or `DOES_IT_BUILD_PARALLEL_JOBS`).


## Configuration

- `DB_PATH`: Path to SQlite DB to store the results
- `DOES_IT_BUILD_PARALLEL_JOBS`: Parallel build jobs, defaults to cores/2.

## Deployment

deployed at <https://does-it-build.noratrieb.dev/>
