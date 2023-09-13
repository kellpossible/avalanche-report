# Avalanche Report

A simple self-hosted web server for creating and managing an avalanche forecast for a region, along with accepting public observations.

## Design Considerations

The following are goals that the project is striving to acheive (or has already achieved and needs to maintain) through careful decision making during the design process.

1. Contains all the basics needed to run an avalanche forecasting service website.
2. Accessible.
  * Designed from the ground-up to be translated into multiple languages.
  * Should work with minimal client system requirements.
  * Should work well on a poor internet connection.
3. Reliable.
  * Avalanche forecasts are an important public service.
  * The only likely point of failure should be the host itself.
  * Budget for on-call tech support is limited.
4. Simple and easy to install and operate from an administrative perspective.
  * Avalanche professionals or enthusiasts are for the most part not necessarily professional server administrators or software engineers.
  * From this perspective, the server is a single self-contained binary that is dead easy to install and run on any computer/web server.
  * A docker container is also provided.
  * The database is using [sqlite](https://www.sqlite.org/index.html), which is embedded into the application (no need for a separate service), and the data consists of a single file.
  * Database backup and restore functionality is built into the application.

## Setup

```bash
$ npm install
$ npx tailwindcss --input src/style.css --output dist/style.css
$ cargo build --release
```
