# 1bln Cell Spreadsheet Techdemo

**Note** This repository is archived; we no longer host/maintain the demo on xls.feldera.io. The code is
still fine to be used as a reference to use and run.

This is a tech demo showing incremental computation for a simple, spreadsheet-like application.
The application uses feldera as the DBMS/incremental compute engine, axum as the backend
and egui as the frontend.

The project is split into three components from the root directory:

- The `feldera` directory contains the feldera pipeline (written in Feldera/SQL and some Rust UDF code).
- The `server` directory contains the backend application (written in Rust using the axum webserver).
- The `client` directory contains the frontend application (written in Rust using the egui UI library).

## Local Installation

You'll need a working rust installation to run the project locally.

### Feldera

Install a [feldera instance](https://docs.feldera.com/get-started), or alternatively use the
feldera instance running on `https://try.feldera.com`.
Also, you'll need to install Feldera's CLI tool [fda](https://docs.feldera.com/reference/cli).

Set the `FELDERA_API_KEY` and `FELDERA_HOST` environment variables to access the right feldera instance.
Finally, execute the `deploy.sh` script in the `feldera` directory to deploy the pipeline:

```bash
export FELDERA_API_KEY=apikey:...
export FELDERA_HOST=https://try.feldera.com
cd feldera && bash deploy.sh
```

### Server

Run the `server` application with cargo:

```bash
cd server
cargo run
```

Now the backend should be running on `http://localhost:3000`. The backend will connect
to your feldera instance to fetch the data. The server uses the `FELDERA_API_KEY` and `FELDERA_HOST`
environment variables set earlier, make sure they're still set correctly.

### Client

Run the `client` application with trunk:

```bash
cd client
API_HOST=http://localhost:3000 trunk serve --port 7777
```

Now the frontend should be running on `http://localhost:7777`. The frontend will connect
to the backend to fetch the data. The `API_HOST` environment variable is set to point to the
backend running on `http://localhost:3000`.

## Automated Deployment with Github Actions

The project is set up to deploy the server backend to [fly.io](https://fly.io/)
and the client application is published using github pages.

## Feldera

Make sure to set the `FELDERA_API_KEY` and `FELDERA_HOST` secrets in the github repository settings.

## Server

Get a fly.io account and install the [fly CLI tool](https://fly.io/docs/flyctl/install/).

Next make sure to set the `FELDERA_API_KEY` and `FELDERA_HOST` secrets also in your fly.io application.

```bash
cd server
fly login
fly secrets set FELDERA_HOST=https://try.feldera.com
fly secrets set FELDERA_API_KEY=apikey:...
```

Finally, you'll need to get an API token from fly.io and set it as a secret named `FLY_API_TOKEN` in the github
repository settings.

## Client

Make sure to set the `API_HOST` secret in the github repository settings to point to your fly.io application URL.
Enable github pages, set the source to `Github Actions`. Then adjust the `public_url` env variable in the `client.yml`
github action file to point to your github pages URL.
