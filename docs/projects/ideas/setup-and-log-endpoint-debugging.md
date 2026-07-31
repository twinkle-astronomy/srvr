# Better Debugging for Setup and Logging Endpoints

Operators self-hosting the server can't currently tell what a device sent during setup, or what its submitted logs actually contained. Troubleshooting a device that fails to provision, or whose logs seem to go missing, means guessing rather than looking.

## Why

When a device fails to complete setup, or logs it submitted don't show up where an operator expects, there's no way to see what actually happened without reproducing the request or digging directly into the database. Server output today doesn't reliably reflect what a specific device did, and there's no way to connect one device's setup, polling, and log activity into a single trail — just scattered, disconnected events.

## What this enables

- An operator can see what a device sent during setup and understand why it succeeded or failed.
- An operator can see the actual content of logs a device submitted, not just that some arrived, without needing direct database access.
- An operator can follow one device's setup, polling, and log activity as a connected sequence rather than unrelated log lines.
- Troubleshooting output can be shared (e.g. in a bug report) without exposing device credentials or tokens.
