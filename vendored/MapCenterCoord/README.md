# Leaflet.MapCenterCoord

A Leaflet control to display the coordinates of the map center, especially useful on **touch/mobile devices**.

The initial idea was based on the plugin [Leaflet.MousePosition](https://github.com/ardhi/Leaflet.MousePosition).

## Requirements

Leaflet v7.x  
Tested ok on Leaflet 1.0 beta2  

#### Some tested platforms

Android 4.4.2: Google Chrome 47 and stock browser  
iOS 9.2: Safari  

Desktops: Windows 7, Windows 10, Ubuntu 14.04 LTS  
Google Chrome 47, Firefox 43, Microsoft Internet Explorer 10, 11 and Edge 25  

## Examples

http://xguaita.github.io/Leaflet.MapCenterCoord/

Sources in [/examples](/xguaita/Leaflet.MapCenterCoord/tree/master/examples) folder

## Install

  + [Download](https://github.com/xguaita/Leaflet.MapCenterCoord/releases/latest) latest release  
    - Source code in `/src` folder  
    - Minified versions in `/dist`  

  + Include js and css files located in the `/dist` folder (production) or `/src` (development)

  + If you want to change the code and build minified versions:  
    First, install node.js on your system. Then run `npm install` to build the minified js and css into `/dist`

## Usage

Add the following line to your map initialization:

``` js
L.control.mapCenterCoord().addTo(map);
```
You can also pass a configuration.


## Configuration options

The MapCenterCoord control inherits options from [Leaflet Controls](http://leafletjs.com/reference.html#control-options).

| Option | Type | Default | Description
| --- | --- | --- | ---
| position | String | `'bottomleft'` | The standard Leaflet.Control [position parameter](http://leafletjs.com/reference.html#control-positions)
| icon | Boolean | `true` | Shows crosshair icon on map center
| onMove | Boolean | `false` | Update the coordinates of the map center while dragging
| template | String | <code>'{y} &#124; {x}'</code> | Display template
| latlngFormat | String | `'DD'` | Show lat/lng coordinates in 3 possible formats:<br>Decimal degrees `'DD'` (DDD.DDDDD°)<br>Degrees and decimal minutes `'DM'` (DDD° MM.MMM')<br>Degrees, minutes and seconds `'DMS'` (DDD° MM' SS.S")
| latlngDesignators | Boolean | `false` | Show N-S and E-W
| projected | Boolean | `false` | Show projected coordinates
| formatProjected | String | `'#.##0,0'` | Number format mask for projected coordinates.<br>[JavaScript Number Formatter](https://code.google.com/p/javascript-number-formatter/) project: [sample/help page](http://www.integraxor.com/developer/codes/js-formatter/format-sample.htm )
| latLngFormatter | Function | `undefined` | Function that takes the lattitude and longitude as arguments and returns a single formatted string, e.g.<pre>function (lat, lng) {<br>    return lat + " north and " + lng + " west";<br>}</pre>


## License

This software is released under the [MIT license](http://opensource.org/licenses/mit-license.php).
