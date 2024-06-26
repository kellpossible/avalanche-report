use std::path::Path;

use fs_extra::{dir, file};

fn deploy_file(from: impl AsRef<Path>, to: impl AsRef<Path>) {
    file::copy(
        from,
        to,
        &file::CopyOptions {
            overwrite: true,
            ..file::CopyOptions::default()
        },
    )
    .unwrap();
}

pub fn main() {
    let dist_dir = Path::new("./dist");
    if !dist_dir.exists() {
        std::fs::create_dir(dist_dir).unwrap();
    }

    deploy_file(
        "./node_modules/uplot/dist/uPlot.iife.min.js",
        dist_dir.join("uPlot.js"),
    );
    deploy_file(
        "./node_modules/uplot/dist/uPlot.min.css",
        dist_dir.join("uPlot.css"),
    );
    deploy_file(
        "./node_modules/htmx.org/dist/htmx.min.js",
        dist_dir.join("htmx.js"),
    );
    deploy_file(
        "./node_modules/leaflet/dist/leaflet.js",
        dist_dir.join("leaflet.js"),
    );
    deploy_file(
        "./node_modules/leaflet/dist/leaflet.css",
        dist_dir.join("leaflet.css"),
    );
    deploy_file(
        "./node_modules/leaflet-gesture-handling/dist/leaflet-gesture-handling.css",
        dist_dir.join("leaflet-gesture-handling.css"),
    );
    deploy_file(
        "./node_modules/leaflet-gesture-handling/dist/leaflet-gesture-handling.js",
        dist_dir.join("leaflet-gesture-handling.js"),
    );
    deploy_file(
        "./node_modules/@maptiler/leaflet-maptilersdk/leaflet-maptilersdk.js",
        dist_dir.join("leaflet-maptilersdk.js"),
    );
    deploy_file(
        "./node_modules/@maptiler/sdk/dist/maptiler-sdk.umd.js",
        dist_dir.join("maptiler-sdk.umd.js"),
    );
    deploy_file(
        "./node_modules/@maptiler/sdk/dist/maptiler-sdk.css",
        dist_dir.join("maptiler-sdk.css"),
    );
    fs_extra::dir::copy(
        "./node_modules/leaflet/dist/images/",
        dist_dir,
        &dir::CopyOptions {
            overwrite: true,
            ..dir::CopyOptions::default()
        },
    )
    .unwrap();

    deploy_file(
        "./node_modules/leaflet-geotag-photo/dist/Leaflet.GeotagPhoto.min.js",
        dist_dir.join("Leaflet.GeotagPhoto.js"),
    );
    deploy_file(
        "./node_modules/leaflet-geotag-photo/dist/Leaflet.GeotagPhoto.css",
        dist_dir.join("Leaflet.GeotagPhoto.css"),
    );
    fs_extra::dir::copy(
        "./node_modules/leaflet-geotag-photo/images/",
        dist_dir,
        &dir::CopyOptions {
            overwrite: true,
            ..dir::CopyOptions::default()
        },
    )
    .unwrap();
    deploy_file(
        "./vendored/MapCenterCoord/L.Control.MapCenterCoord.min.js",
        dist_dir.join("L.Control.MapCenterCoord.js"),
    );
    deploy_file(
        "./vendored/MapCenterCoord/L.Control.MapCenterCoord.min.css",
        dist_dir.join("L.Control.MapCenterCoord.css"),
    );
    fs_extra::dir::copy(
        "./vendored/MapCenterCoord/icons/",
        dist_dir,
        &dir::CopyOptions {
            overwrite: true,
            ..dir::CopyOptions::default()
        },
    )
    .unwrap();
}
