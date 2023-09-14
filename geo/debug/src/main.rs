use plotly::{Image, Configuration};
use show_image::{create_window, ImageView, ImageInfo, WindowOptions};


#[show_image::main]
pub fn main() -> eyre::Result<()> {
    let pixels = geo::render_elevations("../fixtures/ASTGTMV003_N42E044_dem.tif")?;
    // let pixels_rgb = pixels.mapv(|value| (value, value, value));
    // let mut plot = plotly::Plot::new();
    // let image = Image::new(pixels_rgb).color_model(plotly::image::ColorModel::RGB);
    // plot.add_trace(image);
    // plot.set_configuration(Configuration::default().fill_frame(true));
    // plot.show();


    let shape = pixels.shape();
    let view = ImageView::new(
        ImageInfo::mono8(shape[0] as u32, shape[1] as u32),
        pixels.as_slice_memory_order().unwrap(),
    );
    let window = create_window("Mountains", WindowOptions::default()).unwrap();
    window.set_image("mountains", view).unwrap();
    window.wait_until_destroyed()?;
    Ok(())
}

