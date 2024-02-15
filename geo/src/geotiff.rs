//! Code to read a GeoTIFF file using [`tiff`].

use eyre::{bail, Context, ContextCompat};
use ndarray::Array2;
use num_traits::{FromPrimitive, ToPrimitive};
use std::{fs::File, path::Path};
use tiff::tags::Tag;

#[derive(Debug)]
enum Value {
    Short(u64),
    Double(f64),
    Ascii(String),
}

impl Value {
    pub fn as_short(&self) -> Option<u64> {
        if let Self::Short(value) = self {
            Some(*value)
        } else {
            None
        }
    }

    pub fn into_short(self) -> Option<u64> {
        if let Self::Short(value) = self {
            Some(value)
        } else {
            None
        }
    }

    pub fn as_double(&self) -> Option<f64> {
        if let Self::Double(value) = self {
            Some(*value)
        } else {
            None
        }
    }

    pub fn into_double(self) -> Option<f64> {
        if let Self::Double(value) = self {
            Some(value)
        } else {
            None
        }
    }

    pub fn as_ascii(&self) -> Option<&str> {
        if let Self::Ascii(value) = self {
            Some(value)
        } else {
            None
        }
    }

    pub fn into_ascii(self) -> Option<String> {
        if let Self::Ascii(value) = self {
            Some(value)
        } else {
            None
        }
    }
}

/// GeoTIFF GeoKey ID's may take any value between 0 and 65535. Following TIFF general approach,
/// the GeoKey ID's from 32768 and above are available for private implementations. However, no
/// registry will be established for these keys or codes, so developers are warned to use them at
/// their own risk.
///
/// The Key ID's from 0 to 32767 are reserved for use by the official GeoTIFF spec, and are broken
/// down into the following sub-domains:
///
/// ```txt
/// [    0,  1023]       Reserved
/// [ 1024,  2047]       GeoTIFF Configuration Keys
/// [ 2048,  3071]       Geographic/Geocentric CS Parameter Keys
/// [ 3072,  4095]       Projected CS Parameter Keys
/// [ 4096,  5119]       Vertical CS Parameter Keys
/// [ 5120, 32767]       Reserved
/// [32768, 65535]       Private use
/// ```
///
/// GeoKey codes, like keys and tags, also range from 0 to 65535. Following the TIFF approach, all
/// codes from 32768 and above are available for private user implementation. There will be no
/// registry for these codes, however, and so developers must be sure that these tags will only be
/// used internally. Use private codes at your own risk.
///
/// The codes from 0 to 32767 for all public GeoKeys are reserved by this GeoTIFF specification.
///
/// ## Common Public Code Values
///
/// For consistency, several key codes have the same meaning in all implemented GeoKeys
/// possessing a SHORT numerical coding system:
///
/// 0 = undefined
/// 32767 = user-defined
///
/// The "undefined" code means that this parameter is intentionally omitted, for
/// whatever reason. For example, the datum used for a given map may be unknown,
/// or the accuracy of a aerial photo is so low that to specify a particular
/// datum would imply a higher accuracy than is in the data.
///
/// The "user-defined" code means that a feature is not among the standard list,
/// and is being explicitly defined. In cases where this is meaningful, Geokey
/// parameters have been supplied for the user to define this feature.
///
/// "User-Defined" requirements: In each section below a specification of the
/// additional GeoKeys required for the "user-defined" option is given. In all
/// cases the corresponding "Citation" key is strongly recommended, as per the
/// FGDC Metadata standard regarding "local" types.
#[derive(num_derive::FromPrimitive)]
enum KeyId {
    /// See [`GeotiffKey::GTModelTypeGeoKey`].
    GTModelTypeGeoKey = 1024,
    /// See [`GeotiffKey::GTRasterTypeGeoKey`].
    GTRasterTypeGeoKey = 1025,
    /// See [`GeotiffKey::GeographicTypeGeoKey`].
    GeographicTypeGeoKey = 2048,
    /// See [`GeotiffKey::GeogCitationGeoKey`].
    GeogCitationGeoKey = 2049,
    /// See [`GeotiffKey::GeogAngularUnitsGeoKey`].
    GeogAngularUnitsGeoKey = 2054,
    /// See [`GeotiffKey::GeogSemiMajorAxisGeoKey`].
    GeogSemiMajorAxisGeoKey = 2057,
    /// See [`GeotiffKey::GeogInvFlatteningGeoKey`].
    GeogInvFlatteningGeoKey = 2059,
}

/// Note: Use of "user-defined" or "undefined" raster codes is not recommended.
#[derive(num_derive::FromPrimitive, Debug)]
enum RasterType {
    Undefined = 0,
    RasterPixelIsArea = 1,
    RasterPixelIsPoint = 2,
    UserDefined = 32767,
}

/// GeoTIFF defined CS Model Type Codes.
//
/// Notes:
///    1. Geographic and Projected
///       correspond to the FGDC metadata Geographic and
///       Planar-Projected coordinate system types.
///
/// Ranges:
/// ```txt
/// 0              = undefined
/// [   1,  32766] = GeoTIFF Reserved Codes
/// 32767          = user-defined
/// [32768, 65535] = Private User Implementations
/// ```
#[derive(num_derive::FromPrimitive, Debug)]
enum ModelType {
    Undefined = 0,
    /// Projection Coordinate System.
    Projected = 1,
    /// Geographic latitude-longitude System.
    Geographic = 2,
    /// Geocentric (X,Y,Z) Coordinate System.
    ModelTypeGeocentric = 3,
    UserDefined = 32767,
}

/// Note: A Geographic coordinate system consists of both a datum and a Prime Meridian. Some of the
/// names are very similar, and differ only in the Prime Meridian, so be sure to use the correct
/// one. The codes beginning with GCSE_xxx are unspecified GCS which use ellipsoid (xxx); it is
/// recommended that only the codes beginning with GCS_ be used if possible.
///
/// Ranges:
/// ```txt
/// 0 = undefined
/// [    1,  1000] = Obsolete EPSG/POSC Geographic Codes
/// [ 1001,  3999] = Reserved by GeoTIFF
/// [ 4000, 4199]  = EPSG GCS Based on Ellipsoid only
/// [ 4200, 4999]  = EPSG GCS Based on EPSG Datum
/// [ 5000, 32766] = Reserved by GeoTIFF
/// 32767          = user-defined GCS
/// [32768, 65535] = Private User Implementations
/// ```
#[derive(num_derive::FromPrimitive, num_derive::ToPrimitive, Debug)]
#[allow(non_camel_case_types)]
enum GeographicCoordinateSystemType {
    // Note: Geodetic datum using Greenwich PM have codes equal to
    //   the corresponding Datum code - 2000.
    GCS_Adindan = 4201,
    GCS_AGD66 = 4202,
    GCS_AGD84 = 4203,
    GCS_Ain_el_Abd = 4204,
    GCS_Afgooye = 4205,
    GCS_Agadez = 4206,
    GCS_Lisbon = 4207,
    GCS_Aratu = 4208,
    GCS_Arc_1950 = 4209,
    GCS_Arc_1960 = 4210,
    GCS_Batavia = 4211,
    GCS_Barbados = 4212,
    GCS_Beduaram = 4213,
    GCS_Beijing_1954 = 4214,
    GCS_Belge_1950 = 4215,
    GCS_Bermuda_1957 = 4216,
    GCS_Bern_1898 = 4217,
    GCS_Bogota = 4218,
    GCS_Bukit_Rimpah = 4219,
    GCS_Camacupa = 4220,
    GCS_Campo_Inchauspe = 4221,
    GCS_Cape = 4222,
    GCS_Carthage = 4223,
    GCS_Chua = 4224,
    GCS_Corrego_Alegre = 4225,
    GCS_Cote_d_Ivoire = 4226,
    GCS_Deir_ez_Zor = 4227,
    GCS_Douala = 4228,
    GCS_Egypt_1907 = 4229,
    GCS_ED50 = 4230,
    GCS_ED87 = 4231,
    GCS_Fahud = 4232,
    GCS_Gandajika_1970 = 4233,
    GCS_Garoua = 4234,
    GCS_Guyane_Francaise = 4235,
    GCS_Hu_Tzu_Shan = 4236,
    GCS_HD72 = 4237,
    GCS_ID74 = 4238,
    GCS_Indian_1954 = 4239,
    GCS_Indian_1975 = 4240,
    GCS_Jamaica_1875 = 4241,
    GCS_JAD69 = 4242,
    GCS_Kalianpur = 4243,
    GCS_Kandawala = 4244,
    GCS_Kertau = 4245,
    GCS_KOC = 4246,
    GCS_La_Canoa = 4247,
    GCS_PSAD56 = 4248,
    GCS_Lake = 4249,
    GCS_Leigon = 4250,
    GCS_Liberia_1964 = 4251,
    GCS_Lome = 4252,
    GCS_Luzon_1911 = 4253,
    GCS_Hito_XVIII_1963 = 4254,
    GCS_Herat_North = 4255,
    GCS_Mahe_1971 = 4256,
    GCS_Makassar = 4257,
    GCS_EUREF89 = 4258,
    GCS_Malongo_1987 = 4259,
    GCS_Manoca = 4260,
    GCS_Merchich = 4261,
    GCS_Massawa = 4262,
    GCS_Minna = 4263,
    GCS_Mhast = 4264,
    GCS_Monte_Mario = 4265,
    GCS_M_poraloko = 4266,
    GCS_NAD27 = 4267,
    GCS_NAD_Michigan = 4268,
    GCS_NAD83 = 4269,
    GCS_Nahrwan_1967 = 4270,
    GCS_Naparima_1972 = 4271,
    GCS_GD49 = 4272,
    GCS_NGO_1948 = 4273,
    GCS_Datum_73 = 4274,
    GCS_NTF = 4275,
    GCS_NSWC_9Z_2 = 4276,
    GCS_OSGB_1936 = 4277,
    GCS_OSGB70 = 4278,
    GCS_OS_SN80 = 4279,
    GCS_Padang = 4280,
    GCS_Palestine_1923 = 4281,
    GCS_Pointe_Noire = 4282,
    GCS_GDA94 = 4283,
    GCS_Pulkovo_1942 = 4284,
    GCS_Qatar = 4285,
    GCS_Qatar_1948 = 4286,
    GCS_Qornoq = 4287,
    GCS_Loma_Quintana = 4288,
    GCS_Amersfoort = 4289,
    GCS_RT38 = 4290,
    GCS_SAD69 = 4291,
    GCS_Sapper_Hill_1943 = 4292,
    GCS_Schwarzeck = 4293,
    GCS_Segora = 4294,
    GCS_Serindung = 4295,
    GCS_Sudan = 4296,
    GCS_Tananarive = 4297,
    GCS_Timbalai_1948 = 4298,
    GCS_TM65 = 4299,
    GCS_TM75 = 4300,
    GCS_Tokyo = 4301,
    GCS_Trinidad_1903 = 4302,
    GCS_TC_1948 = 4303,
    GCS_Voirol_1875 = 4304,
    GCS_Voirol_Unifie = 4305,
    GCS_Bern_1938 = 4306,
    GCS_Nord_Sahara_1959 = 4307,
    GCS_Stockholm_1938 = 4308,
    GCS_Yacare = 4309,
    GCS_Yoff = 4310,
    GCS_Zanderij = 4311,
    GCS_MGI = 4312,
    GCS_Belge_1972 = 4313,
    GCS_DHDN = 4314,
    GCS_Conakry_1905 = 4315,
    GCS_WGS_72 = 4322,
    GCS_WGS_72BE = 4324,
    GCS_WGS_84 = 4326,
    GCS_Bern_1898_Bern = 4801,
    GCS_Bogota_Bogota = 4802,
    GCS_Lisbon_Lisbon = 4803,
    GCS_Makassar_Jakarta = 4804,
    GCS_MGI_Ferro = 4805,
    GCS_Monte_Mario_Rome = 4806,
    GCS_NTF_Paris = 4807,
    GCS_Padang_Jakarta = 4808,
    GCS_Belge_1950_Brussels = 4809,
    GCS_Tananarive_Paris = 4810,
    GCS_Voirol_1875_Paris = 4811,
    GCS_Voirol_Unifie_Paris = 4812,
    GCS_Batavia_Jakarta = 4813,
    GCS_ATF_Paris = 4901,
    GCS_NDG_Paris = 4902,
    // Ellipsoid-Only GCS:
    //    Note: the numeric code is equal to the code of the correspoding
    //    EPSG ellipsoid, minus 3000.
    GCSE_Airy1830 = 4001,
    GCSE_AiryModified1849 = 4002,
    GCSE_AustralianNationalSpheroid = 4003,
    GCSE_Bessel1841 = 4004,
    GCSE_BesselModified = 4005,
    GCSE_BesselNamibia = 4006,
    GCSE_Clarke1858 = 4007,
    GCSE_Clarke1866 = 4008,
    GCSE_Clarke1866Michigan = 4009,
    GCSE_Clarke1880_Benoit = 4010,
    GCSE_Clarke1880_IGN = 4011,
    GCSE_Clarke1880_RGS = 4012,
    GCSE_Clarke1880_Arc = 4013,
    GCSE_Clarke1880_SGA1922 = 4014,
    GCSE_Everest1830_1937Adjustment = 4015,
    GCSE_Everest1830_1967Definition = 4016,
    GCSE_Everest1830_1975Definition = 4017,
    GCSE_Everest1830Modified = 4018,
    GCSE_GRS1980 = 4019,
    GCSE_Helmert1906 = 4020,
    GCSE_IndonesianNationalSpheroid = 4021,
    GCSE_International1924 = 4022,
    GCSE_International1967 = 4023,
    GCSE_Krassowsky1940 = 4024,
    GCSE_NWL9D = 4025,
    GCSE_NWL10D = 4026,
    GCSE_Plessis1817 = 4027,
    GCSE_Struve1860 = 4028,
    GCSE_WarOffice = 4029,
    GCSE_WGS84 = 4030,
    GCSE_GEM10C = 4031,
    GCSE_OSU86F = 4032,
    GCSE_OSU91A = 4033,
    GCSE_Clarke1880 = 4034,
    GCSE_Sphere = 4035,
}

impl GeographicCoordinateSystemType {
    pub fn to_code(&self) -> String {
        let code_number = ToPrimitive::to_i16(self).expect("Expect code to convert to u16");
        format!("ESPG:{code_number}")
    }
}

/// These codes shall be used for any key that requires specification of an angular unit of
/// measurement.
#[derive(num_derive::FromPrimitive, Debug)]
enum AngularUnits {
    AngularRadian = 9101,
    AngularDegree = 9102,
    AngularArcMinute = 9103,
    AngularArcSecond = 9104,
    AngularGrad = 9105,
    AngularGon = 9106,
    AngularDMS = 9107,
    AngularDMSHemisphere = 9108,
}
#[derive(Debug)]
enum GeoKey {
    /// This GeoKey defines the general type of model Coordinate system used, and to which the
    /// raster space will be transformed:unknown, Geocentric (rarely used), Geographic, Projected
    /// Coordinate System, or user-defined. If the coordinate system is a PCS, then only the PCS
    /// code need be specified. If the coordinate system does not fit into one of the standard
    /// registered PCS'S, but it uses one of the standard projections and datums, then its should
    /// be documented as a PCS model with "user-defined" type, requiring the specification of
    /// projection parameters, etc.
    ///
    /// GeoKey requirements for User-Defined Model Type (not advisable):
    ///      GTCitationGeoKey
    ///
    /// Key ID = 1024
    /// Type: SHORT (code)
    GTModelTypeGeoKey(ModelType),
    /// This establishes the Raster Space coordinate system used; there are currently only two,
    /// namely RasterPixelIsPoint and RasterPixelIsArea. No user-defined raster spaces are
    /// currently supported. For variance in imaging display parameters, such as pixel
    /// aspect-ratios, use the standard TIFF 6.0 device-space tags instead.
    ///
    /// Key ID = 1025  
    GTRasterTypeGeoKey(RasterType),
    /// This key may be used to specify the code for the geographic coordinate system used to map
    /// lat-long to a specific ellipsoid over the earth.
    ///
    /// GeoKey Requirements for User-Defined geographic CS:
    ///
    ///    GeogCitationGeoKey
    ///    GeogGeodeticDatumGeoKey
    ///     GeogAngularUnitsGeoKey (if not degrees)
    ///     GeogPrimeMeridianGeoKey (if not Greenwich)
    ///
    /// Key ID = 2048
    /// Type = SHORT (code)
    GeographicTypeGeoKey(GeographicCoordinateSystemType),
    /// General citation and reference for all Geographic CS parameters.
    ///
    /// Key ID = 2049
    /// Type = ASCII
    /// Values = text
    GeogCitationGeoKey(String),
    /// Allows the definition of user-defined angular geographic units, as measured in radians.
    ///
    /// Key ID = 2055
    /// Type = DOUBLE
    /// Units: radians
    GeogAngularUnitsGeoKey(AngularUnits),
    /// Allows the specification of user-defined Ellipsoid Semi-Major Axis (a).
    ///
    /// Key ID = 2057
    /// Type = DOUBLE
    /// Units: Geocentric CS Linear Units
    GeogSemiMajorAxisGeoKey(f64),
    /// Allows the specification of the inverse of user-defined Ellipsoid's flattening parameter
    /// (f). The eccentricity-squared e^2 of the ellipsoid is related to the non-inverted f by:
    ///
    /// `e^2  = 2*f  - f^2`
    ///
    /// Note: if the ellipsoid is spherical the inverse-flattening becomes infinite; use the
    /// GeogSemiMinorAxisGeoKey instead, and set it equal to the semi-major axis length.
    ///
    /// Key ID = 2059
    /// Type = DOUBLE
    /// Units: none.
    GeogInvFlatteningGeoKey(f64),
}

#[derive(Debug)]
struct GeotiffHeader {
    key_directory_version: u64,
    key_revision: u64,
    minor_revision: u64,
    n_keys: u64,
}

impl TryFrom<&[u64]> for GeotiffHeader {
    type Error = eyre::Error;
    fn try_from(value: &[u64]) -> Result<Self, Self::Error> {
        if value.len() != 4 {
            bail!("Unknown length header array: {}", value.len());
        }
        Ok(Self {
            key_directory_version: value[0],
            key_revision: value[1],
            minor_revision: value[2],
            n_keys: value[3],
        })
    }
}

pub fn load(path: impl AsRef<Path>) -> eyre::Result<Array2<u8>> {
    let f = File::open(path.as_ref())?;
    let mut t = tiff::decoder::Decoder::new(f)?;
    let ascii_params: String = t.get_tag_ascii_string(Tag::GeoAsciiParamsTag)?;
    let ascii_params_bytes = ascii_params.as_bytes();

    let double_params: Vec<f64> = t.get_tag_f64_vec(Tag::GeoDoubleParamsTag)?;
    println!("ascii_params: {ascii_params:?}");
    println!("double_params: {double_params:?}");

    // http://geotiff.maptools.org/spec/geotiff2.4.html#2.4
    let key_directory = t.get_tag_u64_vec(Tag::GeoKeyDirectoryTag)?;
    println!("tag:");

    if key_directory.len() % 4 != 0 {
        bail!("GeoKeyDirectoryTag has an invalid length");
    }
    let mut rows = key_directory.chunks(4);
    let header = rows
        .next()
        .wrap_err("No header row in GeoKeyDirectoryTag")?;
    let header = GeotiffHeader::try_from(header)?;
    dbg!(&header);

    let keys: Vec<GeoKey> = rows
        .map(|key| {
            let id = key[0];
            let location = key[1];
            let count: usize = key[2]
                .try_into()
                .wrap_err("Unable to represent count as usize")?;
            let value_offset_value: u64 = key[3];

            let value = if location == 0 {
                Value::Short(value_offset_value)
            } else {
                let value_offset: usize = value_offset_value
                    .try_into()
                    .wrap_err("Unable to represent value_offset as usize")?;
                match Tag::from_u16(location.try_into()?)
                    .wrap_err_with(|| format!("Unable to parse location as tag {location}"))?
                {
                    Tag::GeoDoubleParamsTag => {
                        let value: f64 = *double_params
                            .get(
                                usize::try_from(value_offset)
                                    .context("Can't convert value_offset into usize")?,
                            )
                            .with_context(|| {
                                format!("No double value for offset {value_offset}")
                            })?;

                        Value::Double(value)
                    }
                    Tag::GeoAsciiParamsTag => {
                        let value_bytes = ascii_params_bytes
                            .get(value_offset..count - 1)
                            .wrap_err("Unable to get ascii bytest with key offset and count")?;

                        let value = String::from_utf8(value_bytes.to_owned())
                            .context("ascii string is not valid utf8")?;

                        Value::Ascii(value)
                    }
                    unexpected => bail!("Unexpected tag referenced: {unexpected:?}"),
                }
            };

            Ok(
                match <KeyId as FromPrimitive>::from_u64(id)
                    .wrap_err_with(|| format!("Unable to parse id {id} as a valid key id"))?
                {
                    KeyId::GeogCitationGeoKey => GeoKey::GeogCitationGeoKey(
                        value.into_ascii().wrap_err("Unexpected value type")?,
                    ),
                    KeyId::GTModelTypeGeoKey => GeoKey::GTModelTypeGeoKey(
                        FromPrimitive::from_u64(
                            value.into_short().wrap_err("Unexpected value type")?,
                        )
                        .wrap_err("Invalid model type")?,
                    ),
                    KeyId::GTRasterTypeGeoKey => GeoKey::GTRasterTypeGeoKey(
                        FromPrimitive::from_u64(
                            value.into_short().wrap_err("Unexpected value type")?,
                        )
                        .wrap_err("Invalid raster type")?,
                    ),
                    KeyId::GeogAngularUnitsGeoKey => GeoKey::GeogAngularUnitsGeoKey(
                        FromPrimitive::from_u64(
                            value.into_short().wrap_err("Unexpected value type")?,
                        )
                        .wrap_err("Invalid angular units")?,
                    ),
                    KeyId::GeographicTypeGeoKey => GeoKey::GeographicTypeGeoKey(
                        FromPrimitive::from_u64(
                            value.into_short().wrap_err("Unexpected value type")?,
                        )
                        .wrap_err("Invalid geographic coordinate system type")?,
                    ),
                    KeyId::GeogSemiMajorAxisGeoKey => GeoKey::GeogSemiMajorAxisGeoKey(
                        value.into_double().wrap_err("Unexpected value type")?,
                    ),
                    KeyId::GeogInvFlatteningGeoKey => GeoKey::GeogInvFlatteningGeoKey(
                        value.into_double().wrap_err("Unexpected value type")?,
                    ),
                },
            )
        })
        .collect::<eyre::Result<_>>()?;

    if header.n_keys != keys.len() as u64 {
        bail!("expected {} keys, found {}", header.n_keys, keys.len());
    }

    dbg!(&keys);

    let model_type = keys
        .iter()
        .find_map(|key| match key {
            GeoKey::GTModelTypeGeoKey(model_type) => Some(model_type),
            _ => None,
        })
        .wrap_err("No model type")?;
    let geographic_type = keys
        .iter()
        .find_map(|key| match key {
            GeoKey::GeographicTypeGeoKey(geographic_type) => Some(geographic_type),
            _ => None,
        })
        .wrap_err("No geographic coordinate system type")?;
    dbg!(geographic_type);

    let model_tie_point_tag = t.get_tag_f64_vec(Tag::ModelTiepointTag)?;
    let model_pixel_scale_tag = t.get_tag_f64_vec(Tag::ModelPixelScaleTag)?;
    let width = t
        .get_tag_u64(Tag::ImageWidth)
        .wrap_err("Unable to get image width")?;
    let height = t
        .get_tag_u64(Tag::ImageLength)
        .wrap_err("Unable to get image length (height)")?;
    dbg!(width);
    dbg!(height);

    // Project from CRS WGS84 to web mercator (I think?)
    let from_proj =
        proj4rs::Proj::from_proj_string(&format!("+proj=longlat +datum=WGS84 +no_defs +type=crs"))?;
    let to_proj = proj4rs::Proj::from_proj_string("+proj=merc +a=6378137 +b=6378137 +lat_ts=0 +lon_0=0 +x_0=0 +y_0=0 +k=1 +units=m +nadgrids=@null +wktext +no_defs +type=crs")?;

    dbg!(t.colortype()?);
    let image = t.read_image()?;
    let image = match image {
        tiff::decoder::DecodingResult::I16(image) => image,
        _ => todo!(),
    };

    let (min, max) = image.iter().fold((i16::MAX, i16::MIN), |mut acc, pixel| {
        if pixel < &acc.0 {
            acc.0 = *pixel;
        }
        if pixel > &acc.1 {
            acc.1 = *pixel;
        }
        acc
    });

    let array = Array2::from_shape_vec([width as usize, height as usize], image)?;

    // Array2::from(image);

    dbg!(&model_tie_point_tag);
    dbg!(&model_pixel_scale_tag);

    for ((x, y), value) in array.indexed_iter() {
        let lat = (x as f64 * model_pixel_scale_tag[0]) + model_tie_point_tag[3];
        let lon = (y as f64 * model_pixel_scale_tag[1]) + model_tie_point_tag[4];
        let alt = *value as f64;
        let mut point = (lat.to_radians(), lon.to_radians(), alt);
        dbg!(point);
        proj4rs::transform::transform(&from_proj, &to_proj, &mut point)?;
        dbg!(point);
    }

    // Map to 256 range
    let image =
        array.mapv(|x| (((x as f64) - (min as f64)) * (256.0 / ((max - min) as f64))) as u8);
    Ok(image)
}
