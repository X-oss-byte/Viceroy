use {
    crate::error::GeoIPConfigError,
    serde_json::{Map, Value as SerdeValue},
    std::{collections::HashMap, fs, net::IpAddr, path::PathBuf, path::Path},
};

#[derive(Clone, Debug)]
pub enum GeoIPMapping {
    Empty,
    InlineToml {
        addresses: HashMap<IpAddr, GeoIPData>,
    },
    Json {
        file: PathBuf,
    },
}

#[derive(Clone, Debug, Default)]
pub struct GeoIPData {
    data: Map<String, SerdeValue>,
}

impl GeoIPData {
    pub fn new() -> Self {
        Self { data: Map::new() }
    }

    pub fn from(data: &Map<String, SerdeValue>) -> Self {
        Self {
            data: data.to_owned(),
        }
    }

    pub fn insert(&mut self, field: String, value: SerdeValue) {
        self.data.insert(field, value);
    }
}

impl ToString for GeoIPData {
    fn to_string(&self) -> String {
        serde_json::to_string(&self.data).unwrap_or_else(|_| "".to_string())
    }
}

mod deserialization {
    use std::{net::IpAddr, str::FromStr};

    use serde_json::Number;

    use {
        super::{GeoIPData, GeoIPMapping},
        crate::error::{FastlyConfigError, GeoIPConfigError},
        serde_json::Value as SerdeValue,
        std::path::PathBuf,
        std::{collections::HashMap, convert::TryFrom},
        toml::value::{Table, Value},
    };

    impl TryFrom<Table> for GeoIPMapping {
        type Error = FastlyConfigError;

        fn try_from(toml: Table) -> Result<Self, Self::Error> {
            fn process_config(mut toml: Table) -> Result<GeoIPMapping, GeoIPConfigError> {
                let format = toml
                    .remove("format")
                    .ok_or(GeoIPConfigError::MissingFormat)
                    .and_then(|format| match format {
                        Value::String(format) => Ok(format),
                        _ => Err(GeoIPConfigError::InvalidFormatEntry),
                    })?;

                let mapping = match format.as_str() {
                    "inline-toml" => process_inline_toml_dictionary(&mut toml)?,
                    "json" => process_json_entries(&mut toml)?,
                    "" => return Err(GeoIPConfigError::EmptyFormatEntry),
                    _ => {
                        return Err(GeoIPConfigError::InvalidGeoIPMappingFormat(
                            format.to_owned(),
                        ))
                    }
                };

                Ok(mapping)
            }

            process_config(toml).map_err(|err| FastlyConfigError::InvalidGeoIPDefinition {
                name: "geoip_mapping".to_string(),
                err,
            })
        }
    }

    pub fn parse_ip_address(address: &str) -> Result<IpAddr, GeoIPConfigError> {
        IpAddr::from_str(address)
            .map_err(|err| GeoIPConfigError::InvalidAddressEntry(err.to_string()))
    }

    fn process_inline_toml_dictionary(toml: &mut Table) -> Result<GeoIPMapping, GeoIPConfigError> {
        fn convert_value_to_json(value: Value) -> Option<SerdeValue> {
            match value {
                Value::String(value) => Some(SerdeValue::String(value)),
                Value::Integer(value) => Number::try_from(value).ok().map(SerdeValue::Number),
                Value::Float(value) => Number::from_f64(value).map(SerdeValue::Number),
                Value::Boolean(value) => Some(SerdeValue::Bool(value)),
                _ => None,
            }
        }

        // Take the `addresses` field from the provided TOML table.
        let toml = match toml
            .remove("addresses")
            .ok_or(GeoIPConfigError::MissingAddresses)?
        {
            Value::Table(table) => table,
            _ => return Err(GeoIPConfigError::InvalidAddressesType),
        };

        let mut addresses = HashMap::<IpAddr, GeoIPData>::with_capacity(toml.len());
        for (address, value) in toml {
            let address = parse_ip_address(address.as_str())?;
            let table = value
                .as_table()
                .ok_or(GeoIPConfigError::InvalidInlineEntryType)?
                .to_owned();

            let mut geoip_data = GeoIPData::new();

            for (field, value) in table {
                let value =
                    convert_value_to_json(value).ok_or(GeoIPConfigError::InvalidInlineEntryType)?;
                geoip_data.insert(field, value);
            }

            addresses.insert(address, geoip_data);
        }

        Ok(GeoIPMapping::InlineToml { addresses })
    }

    fn process_json_entries(toml: &mut Table) -> Result<GeoIPMapping, GeoIPConfigError> {
        let file: PathBuf = match toml.remove("file").ok_or(GeoIPConfigError::MissingFile)? {
            Value::String(file) => {
                if file.is_empty() {
                    return Err(GeoIPConfigError::EmptyFileEntry);
                } else {
                    file.into()
                }
            }
            _ => return Err(GeoIPConfigError::InvalidFileEntry),
        };

        GeoIPMapping::read_json_contents(&file)?;

        Ok(GeoIPMapping::Json { file })
    }
}

impl Default for GeoIPMapping {
    fn default() -> Self {
        GeoIPMapping::new()
    }
}

impl GeoIPMapping {
    pub fn new() -> Self {
        GeoIPMapping::Empty
    }

    pub fn get(&self, address: &IpAddr) -> Option<GeoIPData> {
        match self {
            Self::Empty => None,
            Self::InlineToml { addresses } => addresses.get(address).map(|a| a.to_owned()),
            Self::Json { file } => Self::read_json_contents(file)
                .ok()
                .map(|addresses| addresses.get(address).map(|a| a.to_owned()))
                .unwrap(),
        }
    }

    pub fn read_json_contents(
        file: &Path,
    ) -> Result<HashMap<IpAddr, GeoIPData>, GeoIPConfigError> {
        let data = fs::read_to_string(&file).map_err(GeoIPConfigError::IoError)?;

        // Deserialize the contents of the given JSON file.
        let json = match serde_json::from_str(&data)
            .map_err(|_| GeoIPConfigError::GeoIPFileWrongFormat)?
        {
            // Check that we were given an object.
            serde_json::Value::Object(obj) => obj,
            _ => {
                return Err(GeoIPConfigError::GeoIPFileWrongFormat);
            }
        };

        let mut addresses = HashMap::<IpAddr, GeoIPData>::with_capacity(json.len());
        for (address, value) in json {
            let address = deserialization::parse_ip_address(address.as_str())?;
            let table = value
                .as_object()
                .ok_or(GeoIPConfigError::InvalidInlineEntryType)?
                .to_owned();

            let geoip_data = GeoIPData::from(&table);

            addresses.insert(address, geoip_data);
        }

        Ok(addresses)
    }
}
