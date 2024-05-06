# Avalanche Report [![Crowdin](https://badges.crowdin.net/avalanche-report/localized.svg)](https://crowdin.com/project/avalanche-report) [![github actions badge](https://github.com/kellpossible/avalanche-report/actions/workflows/rust.yml/badge.svg)](https://github.com/kellpossible/avalanche-report/actions?query=workflow%3ARust+branch%3Amain+)

A simple self-hosted web server for creating and managing an avalanche forecast for a region, along with accepting public observations.

Current deployments:

* [avalanche.ge](https://avalanche.ge)
* [bansko.avalanche.bg](https://bansko.avalanche.bg)

Currently it uses a Google Sheet [Avalanche Forecast Template](https://docs.google.com/spreadsheets/d/1vkav8SNr4uv1sOtc6mp2eTDa7nYTj5k852T1rD8F_8Y/edit?usp=sharing) for forecast data entry. Forecasts are placed in a specific google drive folder when they are ready to be published, and are automatically picked up by the server and rendered as HTML to users.

There is a blog post which explains the inception, history and motivations for this project: [Introducing `avalanche-report`](https://lukefrisken.com/code/introducing-avalanche-report/).

## Translations

Translations for the project are happening over at <https://crowdin.com/project/avalanche-report>. Many thanks to [Crowdin](https://crowdin.com/) for supporting this project with an open source license! Contributions are very welcome. If you wish to contribute feel free to contact me <a href="mailto:l.frisken@gmail.com">l.frisken@gmail.com</a>, [Post an Issue](https://github.com/kellpossible/avalanche-report/issues) or [Create a discussion](https://github.com/kellpossible/avalanche-report/discussions). We will need to verify that you have the required technical avalanche knowledge (or close access to) needed to make accurate translations, as this is a safety critical product.

## Design Considerations

The following are goals that the project is striving to acheive (or has already achieved and needs to maintain) through careful decision making during the design process.

1. Self contained with all the basics needed to run an avalanche forecasting service website.
2. Accessible.
  * Designed from the ground-up to be translated into multiple languages.
  * Should work with minimal client system requirements.
  * Should work well on a poor internet connection.
3. Reliable.
  * Avalanche forecasts are an important public service.
  * The only likely point of failure should be the host itself.
  * Budget for on-call tech support is limited.
4. Simple.
  * It should be easy to install and operate from an administrative perspective.
  * Avalanche professionals or enthusiasts are for the most part not necessarily professional server administrators or software engineers.
  * From this perspective, the server is a single self-contained binary that is dead easy to install and run on any computer/web server.
  * A docker container is also provided.
  * The database is using [`sqlite`](https://www.sqlite.org/index.html), which is embedded into the application (no need for a separate service), and the data consists of a single file.
  * Database backup and restore functionality is built into the application.
5. Cost effective.
  * It should be cheap to develop, cheap to deploy, cheap to run.

## Setup

### Building from Source

To build the software yourself should have the following tools installed on your computer and available in your path:

+ [Rust](https://www.rust-lang.org/).
+ [NodeJS](https://nodejs.org/en/).
+ (optional, for development) [just](https://github.com/casey/just) command runner.

The following command will build the software in release mode:

```bash
npm install && \
npx tailwindcss --input src/style.css --output dist/style.css && \
cargo run --release -p migrations && \
DATABASE_URL="sqlite://data/db.sqlite3" cargo build --release
```

The final output is a self-contained static binary at `target/release/avalanche-report` (or `target\release\avalanche-report.exe` on Windows), which you can then take and run. All the required assets are embedded into the binary, this is all you need to run the server.

For convenience, there is also a docker container [Dockerfile](./Dockerfile) based on [`alpine` linux](https://www.alpinelinux.org/) which you can build with:

```bash
$ docker build .
```

### Dockerhub

Docker images for this project are published to dockerhub <https://hub.docker.com/r/lfrisken/avalanche-report> with the name `lfrisken/avalanche-report`. You can use either the `latest` tag, or a tag for a specific git commit version.

See <https://github.com/kellpossible/avalanche-bg-avalanche-report> for an example of customizing and deploying avalanche-report using this dockerhub image.

## Running

### Configuration

Configuration for the `avalanche-report` software makes use of [`toml-env`](https://github.com/kellpossible/toml-env). You can create a `.env.toml` file in your working directory with the following available options, all are optional except those denoted as `(REQUIRED)` in the comment:

```toml
# All values in this top section are also values which can be configured as 
# environment variables with the same name:

# Set the logging.
# Default: `warn,avalanche_report=info`
RUST_LOG="warn,avalanche_report=info"


# Options can be specified by prepending them with `AVALANCHE_REPORT__` and
# with names in uppercase
# See `[AVALANCHE_REPORT]` for available options and their defaults.
# This can be useful for variables like the following which you may want to
# specify using your deployment platform's secret storage capability:
AVALANCHE_REPORT__ADMIN_PASSWORD_HASH="SECRET"
# This variable is within the `backup` scope, so it has an extra `BACKUP__`:
AVALANCHE_REPORT__BACKUP__AWS_SECRET_ACCESS_KEY="SECRET"

# Contains the options in TOML format as a multiline string, normally you
# would use this as an alternative to `.toml.env` file. Options specified
# here are without the `[AVALANCHE_REPORT]` wrapping scope. See `fly.toml`'s
# `env.AVALANCHE_REPORT` key for an example of this. See all values inside 
# `[AVALANCHE_REPORT]` for examples of available configuration options.
AVALANCHE_REPORT="..."

# Available options
[AVALANCHE_REPORT]
# (REQUIRED) Hash of the `admin` user password, used to access `/admin/*` routes.
admin_password_hash="SECRET"
# The default selected langauge for the page (used when the user has not yet
# set a language or when their browser does not provide an Accept-Language header).
default_language="en-UK"
# Directory where application data is stored (including logs).
# Default is `data`.
data_dir="data"
# Address by the http server for listening.
# Default is `127.0.0.1:3000`.
listen_address="127.0.0.1:3000"
# Base url used for http server.
# Can be also specified by setting the environment variable `BASE_URL`.
# Default is `http://{listen_address}/`.
base_url="https://mywebsite.org/"
# The default selected langauge for the page (used when the user has not yet set a language
# or when their browser does not provide an Accept-Language header).
# Default is ["en-UK"]
default_language_order=["en-UK", "ka-GE"]
# Override the default spreadsheet parsing schema, where `area_id` is the id of the forecast area.
forecast_spreadsheet_schema="forecast_spreadsheet_schema.area_id.0.3.1.json"

# Configuration for the HTML templates.
[templates]
# The path to the directory containing overrides for templates.
directory="templates"

# Configuration for application localization.
[i18n]
# The path to the directory containing overrides for localization resources.
directory="i18n"

# (REQUIRED) Configuration for using Google Drive.
[AVALANCHE_REPORT.google_drive]
# (REQUIRED) Google Drive API key, used to access forecast spreadsheets.
api_key="SECRET"
# (REQUIRED) The identifier for the folder in Google Drive where the rublished forecasts are stored.
published_folder_id="your folder id"

# `avalanche-report` has a built-in backup facility which can save the database and push it to an
# amazon s3 compatible storage API.
[AVALANCHE_REPORT.backup]
# Schedule for when analytics data compaction is performed.
# Default is `0 0 * * *` (once per day at 00:00 UTC).
schedule="0 0 * * * *"
aws_secret_access_key="SECRET"
aws_access_key_id="SECRET"
s3_endpoint="https://s3.eu-central-1.amazonaws.com"
s3_bucket_name="my-bucket"
s3_bucket_region="eu-central-1"

# `avalanche-report` has a built-in server-side analytics collection mechanism.
[AVALANCHE_REPORT.analytics]
# Schedule for when analytics data compaction is performed (in cron format).
# Default is `0 1 * * *` (once per day at 01:00 UTC).
compaction_schedule = "0 1 * * *"
# Number of analytics event batches that will be submited to the database per hour.
batch_rate = 60

# Configuration for the map component.
[AVALANCHE_REPORT.map]
# The source for the basemap of the map component.
# This example sets the map source to use https://opentopomap.org, a raster 
# tile source, and the default map source.
# There can only be one `map.source` specified.
# Available options: `OpenTopoMap`, `Ersi`, `MapTiler`, `Tracestrack`.
# Default is `OpenTopoMap`.
source="OpenTopoMap"

# Sets the map source to use https://www.maptiler.com/ which provides vector maps
# with localized place names.
[AVALANCHE_REPORT.map.source.MapTiler]
# Beware that this key is sent to the client, so it's not really a secret.
api_key="SECRET"
# Sets the map style.
# Available options: `topo-v2`, `winter-v2`.
# Default is `winter-v2`.
style="winter-v2"

# Sets the map source to use https://www.tracestrack.com/
[AVALANCHE_REPORT.map.source.Tracestrack]
# Beware that this key is sent to the client, so it's not really a secret.
api_key="SECRET"

# Enables the https://windy.com weather map.
[AVALANCHE_REPORT.weather_maps.Windy]
latitude=42.480
longitude=44.480

# Enables the https://meteoblue.com weather map.
[AVALANCHE_REPORT.weather_maps.Meteoblue]
location_id="gudauri_georgia_614410"

# Enables displaying data from https://ambientweather.net/ weather station.
# `kudebi_top` is the id used for the name of the weather station, 
# in the localization id `weather-station-kudebi_top-label`.
[AVALANCHE_REPORT.weather_stations.kudebi_top.source.ambient_weather]
device_mac_address="54:32:04:4B:E5:94"
api_key="SECRET"
application_key="SECRET"
```

Options can also be specified using the `AVALANCHE_REPORT` environment variable, with a multiline string containing all options specified in TOML format. See the [`fly.toml`](./fly.toml)'s `env.AVALANCHE_REPORT` key for an example of this in a deployment.
