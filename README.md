# GHA Dashboard

## Overview

This project is a WebSocket application that displays GitHub Actions Workflow Run information in real time. It fetches the latest Workflow Runs for specified repositories and streams them to clients.

## Key Features

- Periodically fetches Workflow Runs for specified GitHub repositories.
- Sends the fetched Workflow Run information to all connected clients via WebSocket.
- A Workflow Run contains the following information:
  - Repository name (`repositoryName`)
  - ID (`id`)
  - Workflow name (`workflowName`)
  - Display title (`displayTitle`)
  - Event (`event`)
  - Status (`status`)
  - Creation date and time (`createdAt`)
  - Update date and time (`updatedAt`)
  - HTML URL (`htmlUrl`)
- Fetches the 3 most recently pushed repositories.
- Fetches 2 Workflow Runs for each of those repositories.
- After fetching all Workflow Runs for all repositories, it sorts all Workflow Runs by creation date and sends them to the clients.
- After sending to clients, the process of fetching repository Workflow Runs is repeated.
- Workflow Runs are fetched at a rate of once every 12 seconds.
- After fetching Workflow Runs 5 times, the process starts again from fetching the 3 most recently pushed repositories.
- Clients can close the WebSocket connection.

## Architecture

This project adopts a hexagonal architecture.

```
src/
├── main.rs
├── lib.rs
├── domain/
│   ├── models/
│   │   └── run.rs
│   ├── models.rs
│   ├── external_apis/
│   │   └── github.rs
│   └── external_apis.rs
├── domain.rs
├── application/
│   ├── use_cases/
│   │   └── stream_github_actions_runs.rs
│   ├── use_cases.rs
│   ├── services/
│   └── services.rs
├── application.rs
└── infrastructures/
    └── adapters/
        ├── primary/
        │   ├── web/
        │   └── web.rs
        └── secondary/
            ├── external_apis/
            │   └── github.rs
            └── external_apis.rs
```

- Uses `axum` as the web framework.
- Uses `reqwest` for GitHub API calls.
- Uses `axum::extract::ws` for WebSocket implementation.

## Setup and Execution

### Required Environment Variables

The following environment variables are required to run the project.

- `GITHUB_TOKEN`: Personal access token for accessing the GitHub API.
- `OTEL_EXPORTER_OTLP_ENDPOINT`: Endpoint for the OpenTelemetry Exporter.

### Build Method

```bash
cargo build
```

### Execution Method

```bash
cargo run
```

## API Endpoints

- **WebSocket Endpoint:** `/ws`

## Notes

- Be mindful of GitHub API rate limits.
  - For authenticated requests, the maximum number of requests per hour is 5000.
  - This application makes requests at a rate of once every 12 seconds, resulting in 300 requests per hour.
  - If the rate limit is exceeded, instead of returning an error, it will wait until the next request timing.
