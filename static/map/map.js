const map = L.map('map').setView([42.4758793,44.4751789], 11);

// L.control.mapCenterCoord({
//     onMove: true
// }).addTo(map);
//
var crosshair = L.geotagPhoto.crosshair({ crosshairHTML: '<img src="/dist/images/crosshair.svg" width="100px" />' });
crosshair.addTo(map);

class Position {
    constructor(latitude, longitude) {
        this.latitude = latitude;
        this.longitude = longitude;
    }
}

function updatePositionInput(position) {
    let latitude = position.latitude.toFixed(5);
    let longitude = position.longitude.toFixed(5);
    const positionField = document.getElementById("position");
    positionField.value = `${latitude},${longitude}`;
}

function updatePositionFromCrosshair() {
    const latlng = crosshair.getCrosshairLatLng();
    const position = new Position(latlng.lat, latlng.lng);
    updatePositionInput(position);

}

updatePositionFromCrosshair();

map.on('move', (_) => {
    updatePositionFromCrosshair();
})


// /** Parse position from a string. */
// function parsePosition(positionString) {
//     var ll = location.split(',');
//     if(ll[0] && ll[1]) {
//         return L.latLng(ll);
//     } else {
//         return null;
//     } 
// }


const tileServer = {
    url: "https://api.maptiler.com/maps/winter-v2/{z}/{x}/{y}.png?key=PAwU5jOhvl7JaAABfVB0",
    attribution: "\u003ca href=\"https://www.maptiler.com/copyright/\" target=\"_blank\"\u003e\u0026copy; MapTiler\u003c/a\u003e \u003ca href=\"https://www.openstreetmap.org/copyright\" target=\"_blank\"\u003e\u0026copy; OpenStreetMap contributors\u003c/a\u003e",
    tileSize: 512,
    zoomOffset: -1,
    minZoom: 1,
    crossOrigin: true
};
// const tileServer = {
//     url: "https://mapserver.mapy.cz/turist-m/{z}-{x}-{y}",
//     attribution: '&copy; <a href="https://o.seznam.cz/">Seznam.cz</a>',
// };
// const tileServer = {
//     url: "https://tile.thunderforest.com/outdoors/{z}/{x}/{y}.png?apikey=7bba41b8be27471cbff7ca216b73b91c",
//     attribution: '',
// }; 
// const tileServer = {
//     url: "https://tile.opentopomap.org/{z}/{x}/{y}.png",
//     attribution: 'map data: © <a href="https://openstreetmap.org/copyright">OpenStreetMap</a> contributors, <a href="http://viewfinderpanoramas.org">SRTM</a> | map style: © <a href="https://opentopomap.org">OpenTopoMap</a> (<a href="https://creativecommons.org/licenses/by-sa/3.0/">CC-BY-SA</a>)',
// };
const tiles = L.tileLayer(tileServer.url, {
    maxZoom: 19,
    attribution: tileServer.attribution,
    tileSize: tileServer.tileSize,
    zoomOffset: tileServer.zoomOffset,
    minZoom: tileServer.minZoom,
    crossOrigin: tileServer.crossOrigin
}).addTo(map);

function onEachFeature(feature, layer) {
    if (feature.properties && feature.properties.popupContent) {
        layer.bindPopup(feature.properties.popupContent);
    }
}

fetch("/forecast-areas/gudauri/area.geojson")
    .then(response => response.json())
    .then(geojson => {
        L.geoJSON(geojson, {
            onEachFeature: onEachFeature
        }).addTo(map)
    })
    .catch(err => { throw err });
