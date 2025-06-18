# Analytics Collector

Analytics Collector is a simple, extensible system for collecting and storing analytics events, with a focus on page views and anchor click events. It is composed of a Rust backend server and an optional TypeScript client library for easy integration with web applications.

## Project Structure

- **Rust Backend**: Provides an HTTP API for collecting analytics events, storing them in-memory, and exporting to PostgreSQL. It exposes Prometheus metrics for exporting and monitoring.
- **TypeScript Client**: A small library for capturing page view and anchor click events and sending them to the backend server.

## Getting Started

### Prerequisites

- Rust
- Node.js (for the client library)
- PostgreSQL (optional, for event export)
- Prometheus (optional, for event export and monitoring)

### Running the Rust Backend

1. **Clone the repository:**
   ```bash
   git clone https://github.com/corybuecker/analytics-collector.git
   cd analytics-collector
   ```

2. **Build and run the server:**
   ```bash
   cargo run
   ```

   By default, the backend listens on port 8000. You can configure environment variables as needed. The metrics endpoint runs on port 8001 by default.

3. **Prometheus Configuration Example:**
   Add the following job to your Prometheus configuration to scrape the metrics endpoint:
   ```yaml
   - job_name: "analytics-collector"
     kubernetes_sd_configs:
       - role: pod
     relabel_configs:
       - source_labels:
           - __meta_kubernetes_pod_name
         action: keep
         regex: "analytics-collector-.*"
       - source_labels:
           - __meta_kubernetes_pod_container_port_number
         regex: "8000"
         action: keep
   ```

### Running the TypeScript Client

1. **Install dependencies:**
   ```bash
   cd client
   npm install
   ```

2. **Build the client:**
   ```bash
   npm run build
   ```

3. **Usage:**
   Import the library in your web application and initialize it:
   ```typescript
   import AnalyticsCollector from '@corybuecker/analytics-collector';

   const collector = AnalyticsCollector.initialize('http://localhost:8000', 'your-app-id');
   collector.start();
   ```

   This will automatically capture page views and anchor click events and send them to the backend.

## Event Payload Structure

Example payload for a page view:
```json
{
  "entity": "page",
  "action": "view",
  "ts": "2024-05-06T12:00:00Z",
  "path": "/home",
  "appId": "your-app-id"
}
```

Example payload for an anchor click:
```json
{
  "entity": "anchor",
  "action": "click",
  "appId": "your-app-id"
}
```

Payloads are validated server-side for structure and required fields.

## Environment Variables

The Rust backend can be configured using the following environment variables:

| Variable       | Description                                      | Default |
| -------------- | ------------------------------------------------ | ------- |
| DATABASE_URL   | PostgreSQL connection string. Enables event export to PostgreSQL if set. | _unset_ |
| PORT           | The port the backend server listens on. The Prometheus metrics endpoint runs on `PORT + 1`. | 8000    |

Set these variables in your environment before running the backend as needed.

## Notes

This README was written by AI.

## License

This project is licensed under the MIT License. See the [LICENSE](./LICENSE) file for details.