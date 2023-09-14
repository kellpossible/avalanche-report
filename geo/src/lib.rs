use std::path::Path;

// use gdal::raster::{ResampleAlg, StatisticsAll};
use ndarray::Array2;

pub mod geotiff;


pub fn render_elevations<P: AsRef<Path>>(path: P) -> eyre::Result<Array2<u8>> {
    let image = geotiff::load(path)?;

    // let path = path.as_ref();
    // let dataset = gdal::Dataset::open(
    //     path.as_os_str()
    //         .to_str()
    //         .wrap_err("Unable to convert path to str")?,
    // )
    // .wrap_err("Error opening")?;
    // dbg!(dataset.raster_count());
    // dbg!(dataset.raster_size());
    //
    // let scale = 1.0;
    //
    // let raster = dataset.rasterband(1).wrap_err("Error getting raster")?;
    // let array_size = (
    //     (raster.size().0 as f64 * scale) as usize,
    //     (raster.size().1 as f64 * scale) as usize,
    // );
    // dbg!(array_size);
    // let array = raster
    //     .read_as_array::<i32>((0, 0), raster.size(), array_size, Some(ResampleAlg::Cubic))
    //     .wrap_err("Error readig raster into array")?;
    // dbg!(array.shape());
    // let stats: StatisticsAll = raster
    //     .get_statistics(true, true)?
    //     .wrap_err("No statistics")?;
    //
    // let proj = dataset.projection();
    // let xform = dataset.geo_transform()?;
    // let spatial_ref = dataset.spatial_ref()?;
    // dbg!(&spatial_ref);
    // dbg!(&xform);
    // dbg!(&proj);
    // dbg!(&stats);

    // Map to 256 range
    // let image =
    //     array.mapv(|x| (((x as f64) - (min as f64)) * (256.0 / ((max - min) as f64))) as u8);

    // let image = array.mapv(|h| if h > 1000 && h < 2000 { 255u8 } else { 0u8 });

    Ok(image)
}
