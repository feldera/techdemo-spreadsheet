# 1bln Cell Spreadsheet

This is a tech demo showing incremental computation for a simple, spreadsheet-like application.
The application uses feldera as the DBMS/incremental compute engine, and axum as the backend
and egui as the frontend.

## Local development

You'll need to have rust, `cargo`, `fda` and `trunk` (last two both through `cargo install`)
installed to run this application locally.

Run the backend fom the `server` directory:

```bash
cd server
RUST_LOG=debug FELDERA_KEY='' cargo run
```

To access the feldera instance if already defined in your environment `FELDERA_HOST` and `FELDERA_KEY`, take
precedence. You can unset these variable to use the local instance:

```bash
unset FELDERA_KEY
unset FELDERA_HOST
```

Run the frontend from the `client` directory:

```bash
cd client
trunk serve --port 7777
```

Run the database from the `feldera` directory:

Make sure you have a feldera instance running on `http://localhost:8080`.

```bash
git clone https://github.com/feldera/feldera.git feldera-service
cd feldera-service
cd sql-to-dbsp-compiler && ./build.sh
cd .. && cargo run --package=pipeline-manager --features pg-embed --bin pipeline-manager
```

Make sure the `fda` tool is installed to deploy the SQL to the feldera instance:

```bash
cd feldera
export FELDERA_HOST=http://localhost:3000
export FELDERA_API_KEY=
bash deploy.sh
```

Once all three components are running:

- Open [http://localhost:7777](http://localhost:7777) with your browser to see the spreadsheet.
- Open [http://localhost:8080](http://localhost:8080) with your browser to see the feldera instance.
- Open [http://localhost:3000/api/spreadsheet](http://localhost:3000/api/spreadsheet), or
  [http://localhost:3000/api/stats](http://localhost:3000/api/cellstream) to issue API calls.

## Deployment

- The `server` application is deployed to fly.io it includes and serves the web-assembly artefact built under `client`.
- The `feldera` instance is deployed to run from `try.feldera.com`. An API key is added to the server application
  to access this instance.

```bash
fly secrets set FELDERA_API_KEY=apikey:...
fly secrets set FELDERA_HOST=https://try.feldera.com
```