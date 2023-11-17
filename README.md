# Avalanche Report

A simple self-hosted web server for creating and managing an avalanche forecast for a region, along with accepting public observations.

Currently it uses a Google Sheet [Avalanche Forecast Template](https://docs.google.com/spreadsheets/d/1vkav8SNr4uv1sOtc6mp2eTDa7nYTj5k852T1rD8F_8Y/edit?usp=sharing) for forecast data entry. Forecasts are placed in a specific google drive folder when they are ready to be published, and are automatically picked up by the server and rendered as HTML to users.

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
cargo build --release
```

The final output is a self-contained static binary at `target/release/avalanche-report` (or `target\release\avalanche-report.exe` on Windows), which you can then take and run. All the required assets are embedded into the binary, this is all you need to run the server.

For convenience, there is also a docker container [Dockerfile](./Dockerfile) based on [`alpine` linux](https://www.alpinelinux.org/) which you can build with:

```bash
$ docker build
```

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
```

Options can also be specified using the `AVALANCHE_REPORT` environment variable, with a multiline string containing all options specified in TOML format. See the [`fly.toml`](./fly.toml)'s `env.AVALANCHE_REPORT` key for an example of this in a deployment.

## Project Background

This section is written from my personal perspective (Luke Frisken) as the person who started writing this software to serve as a background for other who are interested in motivations and direction for the project.

The project was started in January 2021 in collaboration with members of the [Georgia Avalanche and Snow Conditions🇬🇪ზვავის და თოვლის შესახებ](https://www.facebook.com/groups/830534093958221) community in order to power [avalanche.ge](https://avalanche.ge). We wanted a platform to allow us to produce and publish avalanche forecasts. Research of the existing options was conducted, I eventuall found some exisiting open source projects:

- [`ac-web`](https://github.com/avalanche-canada/ac-web) (deprecated?)
- [`albina-euregio`](https://gitlab.com/albina-euregio) (hosted on <https://avalanche.report>)

`albina-euregio` seemed like the most promising project to adopt, it seems to be developed and adapted to the needs of the forecast regions it is serving, but if we had an allocated budget for paying someone to do the work to make it useful for Georgia, that's probably what we would have done, and maybe will do in the future. However with a budget of $0 with no interest for funding, and deadline of "now" (season had already started), we had to rely on volunteer work and a desire to achieve minimum viable product, as soon as possible.

For my own part, launching into adopting a complex (but fantastic looking) project like `albina-euregio` felt too much like my day job, and could potentially take many weeks or even months to produce something we could use for a forecast. It also has many components which need to be hosted, which rely on NodeJS (not exactly known for its slim resource consumption), and that hosting and associated maintenance needs to be paid for with our budget of $0. For motivation to remain high, I decided to "re-invent the wheel", and take a different approach. 

I'm personally motivated by a desire to help this community and my love for Georgian mountains, people and culture. I also can see how a project like this could be useful for other communities which also lack the resources to develop their own comprehensive solution or to adopt the existing ones. With this in mind I intend to create a solution from the ground up with a goal to service multiple regions and languages, with minimal cost or effort required to deploy on the part of the users. It's also just fun to work on software projects like this which combine multiple interests, and has real-world applications. It can also serve as a test-bed for many ideas I have about how to improve avalanche forecasting in the future.

We started with the primary goal of minimum viable product in as short a time as possible, and I knew that we needed to iterate quickly in order to establish our forms for entering data. The initial solution I produced was using Google Sheets, yes you heard it correct, a spreadsheet! This has the benefits that it's very fast to iterate, it's completely free, anybody can make contributions, or run it themselves without requiring any techninical knowledge, and it even has live collaboration features. In theory vendor lock-in is reduced with the possibility to export to spreadsheet file (in practice this isn't completely true due to the use of google sheets specific functions).

And so a mother of all avalanche forecasting spreadsheets, [Avalanche Forecast Template](https://docs.google.com/spreadsheets/d/1vkav8SNr4uv1sOtc6mp2eTDa7nYTj5k852T1rD8F_8Y/edit?usp=sharing) was completed in a few weeks. It has localizations for English and Georgian, a sheet for producing the forecast and a sheet formatted for printing to PDF which we could share with our community on Facebook. It also has dynamically generated diagrams which were the inception for the software in this repository, which contains diagram rendering code in [`src/diagrams`](./src/diagrams/), which can render diagrams to png images to be embedded dynamically in the spreadsheet using the [`IMAGE`](https://support.google.com/docs/answer/3093333?hl=en) function.

I chose to implement this software using [Rust](https://www.rust-lang.org/) with a long view for the project, because it's what I use primarily for my day job, and I have also implemented several other similar style personl projects (e.g. [`email-eather`](https://kellpossible.github.io/email-weather/)) with it. The language is well suited to reliable, self-contained software which uses minimal resources to run. It also offers an opportunity to access native libraries like [`GDAL`](https://gdal.org/index.html) for implementing interesting GIS related features in the future. There's something satisfying about crafting efficient systems from a climate perspective too! I realise this does place a barrier for entry for contributors over using something more popular like TypeScript/Javascript, but in my day job we are already migrating to Rust for all new projects or rewrites in preference to TypeScript it is going well!

Once we had a PDF version of the forecast working the first stage of the project was successful. It was evident however that there were several problems with the spreadsheet approach that needed addressing:

+ Forecasters were accidentally making edits which break the spreadsheet.
  + This can be partially be addressed by locking cells, however when forecasters copy the template to create the forecast they can unlock them.
  + Re-ordering the problem types is error-prone.
+ Using the spreadsheet is more difficult than a website and probably requires more training.
+ Producing the PDF is a multi-step process, and forecasters were forgetting to produce the Georgian version, even though that functionality is available.
+ Formatting on the PDF was sometimes a bit wonky.

Most of the friction encountered was on the PDF generation, and sharing via Facebook posts. To this end I embarked on creating a website to replace this role. The first step was to have a way to share forecast PDF files on the website, so I created a "Published" folder in our shared google drive, where published forecasts can be placed, and this software would, when handling a request, fetch available forecasts and present them to the user on the index with links to download the PDF files. With the help of @PSAvalancheConsulting we added a forecast area map, along with highlighted forecast elevations to our home page. This served us well for the remainder of the season. I added some analytics collection capability so we could observe website traffic in relation to published forecasts, and some caching to avoid exceeding the Google Drive API request limits.

With the first season over, we had some time to work on the next stage, publishing forecasts via HTML instead of PDF. This would make the forecasts more accessible on mobile, and reduce the overhead on forecasters for producing PDF files in various languages. Taking a very similar approach, the "Published" folder is re-used, instead of PDF files, forecast spreadsheets are moved here when they are ready to be published on the website. The software parses these sheets to extract only the relevant data, and renders the forecast as HTML, essentially using Google Drive like a CMS, to ammend a forecast one only needs to edit the spreadsheet!

From a technical perspective, the HTML rendering and interaction on the website is mostly achieved through the use of [`HTMX`](https://htmx.org/), instead of using a more popular Javascript single page application framework (SPA) like React. This hearkens back to the older days of the internet when times were simpler, one could spin up a website with a few lines of PHP. This is for the most part, fundamentally a simple website, we present information to users on almost-static pages. Using hyperlinks, HTML forms, and HTMX has allowed me to implement most of this functionality on the backend in a single language (Rust). What little low latency client-only interaction we do need to support can be done with a couple of snippets of Javascript and simple self-contained libraries like [`leafletjs`](https://leafletjs.com/). My working theory is that there is simply no need to introduce the [significant complexity associated with adopting a Single Page Application](https://htmx.org/essays/a-response-to-rich-harris/#the-elephant-in-the-room-complexity) javascript framework approach, nor does this team of 1 developer have the [complexity budget](https://htmx.org/essays/complexity-budget/) available to adopt it. In a sense this project for me serves as a real-world experiment to test that position.

As of this writing (November 2023), the plan is to complete a more professional looking website design before the season begins, with the ability for forecasters to edit content on the home page (such as announcements) without requiring a website re-deploy. We will again make use of the forecast spreasheet template to capitalize on the work done to create it, and retain flexibility if we wish to change it, however the long term goal is to eventually bring the data entry capability over to this server, making this software an all-in-one solution for avalanche forecasting.
