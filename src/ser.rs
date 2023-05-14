//! Integrates with [serde](https://serde.rs/) enabling ['EventRecord`](crate::EventRecord) to be serialized to various formats.
//!
//! Requires the `serde` feature be enabled.
//!
//! If the `time_rs` feature is enabled, then time stamps are serialized per the serialization format
//! of the time crate. Otherwise, if `time_rs` is not enabled, then timestamps are serialized as 64bit
//! unix timestamps.
//!
//! ```
//! use ferrisetw::schema_locator::SchemaLocator;
//! use ferrisetw::{EventRecord, EventSerializer};
//! extern crate serde_json;
//!
//! fn event_callback(record: &EventRecord, schema_locator: &SchemaLocator) {
//!     match schema_locator.event_schema(record) {
//!         Err(err) => println!("Error {:?}", err),
//!         Ok(schema) => {
//!             // Generate a serializer for the record using the schema
//!             let ser = EventSerializer::new(record, &schema, Default::default());
//!             // Pass the serializer to any serde compatible serializer
//!             match serde_json::to_value(ser) {
//!                 Err(err) => println!("Error {:?}", err),
//!                 Ok(json) => println!("{}", json),
//!             }
//!         }
//!     }
//! }
//! ```
#![cfg(feature = "serde")]

use crate::native::etw_types::event_record::EventRecord;
use crate::native::tdh_types::{Property, PropertyInfo, TdhInType, TdhOutType};
use crate::native::time::{FileTime, SystemTime};
use crate::parser::Parser;
use crate::schema::Schema;
use crate::GUID;
use serde::ser::{SerializeMap, SerializeStruct};
use std::net::IpAddr;
use windows::Win32::System::Diagnostics::Etw::{EVENT_DESCRIPTOR, EVENT_HEADER};

/// Serialization options for EventSerializer
#[derive(Clone, Copy)]
pub struct EventSerializerOptions {
    /// Includes information from the schema in the serialized output such as the provider, opcode, and task names.
    pub include_schema: bool,
    /// Includes the [EVENT_HEADER](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-dtyp/fa4f7836-06ee-4ab6-8688-386a5a85f8c5) in the serialized output.
    pub include_header: bool,
    /// Includes the set of [EVENT_HEADER_EXTENDED_DATA_ITEM](https://learn.microsoft.com/en-us/windows/win32/api/evntcons/ns-evntcons-event_header_extended_data_item) in the serialized output.
    pub include_extended_data: bool,
    /// When `true` unimplemented serialization fails with an error, otherwise unimplemented serialization is skipped and will not be present in the serialized output.
    pub fail_unimplemented: bool,
}

impl core::default::Default for EventSerializerOptions {
    fn default() -> Self {
        Self {
            include_schema: true,
            include_header: true,
            include_extended_data: false,
            fail_unimplemented: false,
        }
    }
}

/// Used to serialize ['EventRecord`](crate::EventRecord) using [serde](https://serde.rs/)
pub struct EventSerializer<'a> {
    pub(crate) record: &'a EventRecord,
    pub(crate) schema: &'a Schema,
    pub(crate) parser: Parser<'a, 'a>,
    pub(crate) options: EventSerializerOptions,
}

impl<'a> EventSerializer<'a> {
    /// Creates an event serializer object.
    pub fn new(
        record: &'a EventRecord,
        schema: &'a Schema,
        options: EventSerializerOptions,
    ) -> Self {
        Self {
            record,
            schema,
            parser: Parser::create(record, schema),
            options,
        }
    }
}

impl serde::ser::Serialize for EventSerializer<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut state = serializer.serialize_struct("Record", 4)?;

        if self.options.include_schema {
            let schema = SchemaSer::new(self.schema);
            state.serialize_field("Schema", &schema)?;
        } else {
            state.skip_field("Schema")?;
        }

        if self.options.include_header {
            let header = HeaderSer::new(&self.record.0.EventHeader);
            state.serialize_field("Header", &header)?;
        } else {
            state.skip_field("Header")?;
        }

        if self.options.include_extended_data && self.options.fail_unimplemented {
            // TODO
            return Err(serde::ser::Error::custom(
                "not implemented for extended data",
            ));
        } else {
            state.skip_field("Extended")?;
        }

        let event = EventSer::new(self.record, self.schema, &self.parser, &self.options);
        state.serialize_field("Event", &event)?;

        state.end()
    }
}

struct GUIDExt(GUID);

impl serde::ser::Serialize for GUIDExt {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        if serializer.is_human_readable() {
            return serializer.serialize_str(&format!("{:?}", self.0));
        }

        (self.0.data1, self.0.data2, self.0.data3, self.0.data4).serialize(serializer)
    }
}

struct SchemaSer<'a> {
    schema: &'a Schema,
}

impl<'a> SchemaSer<'a> {
    fn new(schema: &'a Schema) -> Self {
        Self { schema }
    }
}

impl serde::ser::Serialize for SchemaSer<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("Schema", 3)?;
        state.serialize_field("Provider", &self.schema.provider_name().trim())?;
        state.serialize_field("Opcode", &self.schema.opcode_name().trim())?;
        state.serialize_field("Task", &self.schema.task_name().trim())?;
        state.end()
    }
}

struct HeaderSer<'a> {
    header: &'a EVENT_HEADER,
}

impl<'a> HeaderSer<'a> {
    fn new(header: &'a EVENT_HEADER) -> Self {
        Self { header }
    }
}

impl serde::ser::Serialize for HeaderSer<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut state = serializer.serialize_struct("Header", 10)?;
        state.serialize_field("Size", &self.header.Size)?;
        state.serialize_field("HeaderType", &self.header.HeaderType)?;
        state.serialize_field("Flags", &self.header.Flags)?;
        state.serialize_field("EventProperty", &self.header.Flags)?;
        state.serialize_field("ThreadId", &self.header.ThreadId)?;
        state.serialize_field("ProcessId", &self.header.ProcessId)?;
        state.serialize_field("TimeStamp", &FileTime::from_quad(self.header.TimeStamp))?;
        state.serialize_field("ProviderId", &GUIDExt(self.header.ProviderId))?;
        state.serialize_field("ActivityId", &GUIDExt(self.header.ActivityId))?;
        let descriptor = DescriptorSer::new(&self.header.EventDescriptor);
        state.serialize_field("Descriptor", &descriptor)?;
        state.end()
    }
}

struct DescriptorSer<'a> {
    descriptor: &'a EVENT_DESCRIPTOR,
}

impl<'a> DescriptorSer<'a> {
    fn new(descriptor: &'a EVENT_DESCRIPTOR) -> Self {
        Self { descriptor }
    }
}

impl serde::ser::Serialize for DescriptorSer<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut state = serializer.serialize_struct("Descriptor", 7)?;
        state.serialize_field("Id", &self.descriptor.Id)?;
        state.serialize_field("Version", &self.descriptor.Version)?;
        state.serialize_field("Channel", &self.descriptor.Channel)?;
        state.serialize_field("Level", &self.descriptor.Level)?;
        state.serialize_field("Opcode", &self.descriptor.Opcode)?;
        state.serialize_field("Task", &self.descriptor.Task)?;
        state.serialize_field("Keyword", &self.descriptor.Keyword)?;
        state.end()
    }
}

struct EventSer<'a, 'b> {
    record: &'a EventRecord,
    schema: &'a Schema,
    parser: &'a Parser<'b, 'b>,
    options: &'a EventSerializerOptions,
}

impl<'a, 'b> EventSer<'a, 'b> {
    fn new(
        record: &'a EventRecord,
        schema: &'a Schema,
        parser: &'a Parser<'b, 'b>,
        options: &'a EventSerializerOptions,
    ) -> Self {
        Self {
            record,
            schema,
            parser,
            options,
        }
    }
}

impl serde::ser::Serialize for EventSer<'_, '_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut len: usize = 0;
        let props = match self
            .schema
            .try_properties()
            .map_err(serde::ser::Error::custom)
        {
            Err(e) if self.options.fail_unimplemented => return Err(e),
            Ok(p) => p,
            _ => &[],
        };

        for prop in props {
            if prop.get_parser().is_some() {
                len += 1;
            } else if self.options.fail_unimplemented {
                match prop.info {
                    PropertyInfo::Value {
                        in_type, out_type, ..
                    } => {
                        return Err(serde::ser::Error::custom(format!(
                            "not implemented {} in_type: {:?} out_type: {:?}",
                            prop.name, in_type, out_type,
                        )));
                    }
                    PropertyInfo::Array {
                        in_type,
                        out_type,
                        count,
                        ..
                    } => {
                        return Err(serde::ser::Error::custom(format!(
                            "not implemented {} in_type: {:?} out_type: {:?} count: {:?}",
                            prop.name, in_type, out_type, count
                        )));
                    }
                }
            }
        }

        let mut state = serializer.serialize_map(Some(len))?;
        for prop in props {
            if let Some(s) = prop.get_parser() {
                s.0.ser::<S>(&mut state, prop, self.parser, self.record)?;
            }
        }
        state.end()
    }
}

struct PropSer(PropHandler);

trait PropSerable {
    fn get_parser(&self) -> Option<PropSer>;
}

enum PropHandler {
    Null,
    Bool,
    Int8,
    UInt8,
    Int16,
    UInt16,
    Int32,
    UInt32,
    Int64,
    UInt64,
    Pointer,
    Float,
    Double,
    String,
    FileTime,
    SystemTime,
    Guid,
    Binary,
    IpAddr,
    ArrayInt16,
    ArrayUInt16,
    ArrayInt32,
    ArrayUInt32,
    ArrayInt64,
    ArrayUInt64,
    ArrayPointer,
}

macro_rules! prop_ser_type {
    ($typ:ty, $map:expr, $prop:expr, $parser:expr) => {{
        let v = $parser
            .try_parse::<$typ>(&$prop.name)
            .map_err(serde::ser::Error::custom)?;
        $map.serialize_entry(&$prop.name, &v)
    }};
}

impl PropHandler {
    fn ser<S>(
        &self,
        map: &mut S::SerializeMap,
        prop: &Property,
        parser: &Parser,
        record: &EventRecord,
    ) -> Result<(), S::Error>
    where
        S: serde::ser::Serializer,
    {
        match self {
            PropHandler::Bool => prop_ser_type!(bool, map, prop, parser),
            PropHandler::Int8 => prop_ser_type!(i8, map, prop, parser),
            PropHandler::UInt8 => prop_ser_type!(u8, map, prop, parser),
            PropHandler::Int16 => prop_ser_type!(i16, map, prop, parser),
            PropHandler::UInt16 => prop_ser_type!(u16, map, prop, parser),
            PropHandler::Int32 => prop_ser_type!(i32, map, prop, parser),
            PropHandler::UInt32 => prop_ser_type!(u32, map, prop, parser),
            PropHandler::Int64 => prop_ser_type!(i64, map, prop, parser),
            PropHandler::UInt64 => prop_ser_type!(u64, map, prop, parser),
            PropHandler::Float => prop_ser_type!(f32, map, prop, parser),
            PropHandler::Double => prop_ser_type!(f64, map, prop, parser),
            PropHandler::String => prop_ser_type!(String, map, prop, parser),
            PropHandler::Binary => prop_ser_type!(Vec<u8>, map, prop, parser),
            PropHandler::IpAddr => prop_ser_type!(IpAddr, map, prop, parser),
            PropHandler::FileTime => prop_ser_type!(FileTime, map, prop, parser),
            PropHandler::SystemTime => prop_ser_type!(SystemTime, map, prop, parser),
            PropHandler::ArrayInt16 => prop_ser_type!(&[i16], map, prop, parser),
            PropHandler::ArrayUInt16 => prop_ser_type!(&[u16], map, prop, parser),
            PropHandler::ArrayInt32 => prop_ser_type!(&[i32], map, prop, parser),
            PropHandler::ArrayUInt32 => prop_ser_type!(&[u32], map, prop, parser),
            PropHandler::ArrayInt64 => prop_ser_type!(&[i64], map, prop, parser),
            PropHandler::ArrayUInt64 => prop_ser_type!(&[u64], map, prop, parser),
            PropHandler::Null => {
                let value: Option<usize> = None;
                map.serialize_entry(&prop.name, &value)
            }
            PropHandler::Pointer => {
                if record.pointer_size() == 4 {
                    prop_ser_type!(u32, map, prop, parser)
                } else {
                    prop_ser_type!(u64, map, prop, parser)
                }
            }
            PropHandler::ArrayPointer => {
                if record.pointer_size() == 4 {
                    prop_ser_type!(&[u32], map, prop, parser)
                } else {
                    prop_ser_type!(&[u64], map, prop, parser)
                }
            }
            PropHandler::Guid => {
                let guid = parser
                    .try_parse::<GUID>(&prop.name)
                    .map_err(serde::ser::Error::custom)?;
                map.serialize_entry(&prop.name, &GUIDExt(guid))
            }
        }
    }
}

impl PropSerable for PropertyInfo {
    fn get_parser(&self) -> Option<PropSer> {
        // give the output type parser first if there is one, otherwise use the input type
        match self {
            PropertyInfo::Value { in_type, out_type, .. } => {
                match out_type {
                    TdhOutType::OutTypeIpv4 => Some(PropSer(PropHandler::IpAddr)),
                    TdhOutType::OutTypeIpv6 => Some(PropSer(PropHandler::IpAddr)),
                    _ => match in_type {
                        TdhInType::InTypeNull => Some(PropSer(PropHandler::Null)),
                        TdhInType::InTypeUnicodeString => Some(PropSer(PropHandler::String)),
                        TdhInType::InTypeAnsiString => Some(PropSer(PropHandler::String)),
                        TdhInType::InTypeInt8 => Some(PropSer(PropHandler::Int8)),
                        TdhInType::InTypeUInt8 => Some(PropSer(PropHandler::UInt8)),
                        TdhInType::InTypeInt16 => Some(PropSer(PropHandler::Int16)),
                        TdhInType::InTypeUInt16 => Some(PropSer(PropHandler::UInt16)),
                        TdhInType::InTypeInt32 => Some(PropSer(PropHandler::Int32)),
                        TdhInType::InTypeUInt32 => Some(PropSer(PropHandler::UInt32)),
                        TdhInType::InTypeInt64 => Some(PropSer(PropHandler::Int64)),
                        TdhInType::InTypeUInt64 => Some(PropSer(PropHandler::UInt64)),
                        TdhInType::InTypeFloat => Some(PropSer(PropHandler::Float)),
                        TdhInType::InTypeDouble => Some(PropSer(PropHandler::Double)),
                        TdhInType::InTypeBoolean => Some(PropSer(PropHandler::Bool)),
                        TdhInType::InTypeBinary => Some(PropSer(PropHandler::Binary)),
                        TdhInType::InTypeGuid => Some(PropSer(PropHandler::Guid)),
                        TdhInType::InTypePointer => Some(PropSer(PropHandler::Pointer)),
                        TdhInType::InTypeFileTime => Some(PropSer(PropHandler::FileTime)),
                        TdhInType::InTypeSystemTime => Some(PropSer(PropHandler::SystemTime)),
                        TdhInType::InTypeSid => Some(PropSer(PropHandler::String)),
                        TdhInType::InTypeHexInt32 => Some(PropSer(PropHandler::Int32)),
                        TdhInType::InTypeHexInt64 => Some(PropSer(PropHandler::Int64)),
                        TdhInType::InTypeCountedString => None, // TODO
                    },
                }
            }
            PropertyInfo::Array { in_type, .. } => {
                match in_type {
                    TdhInType::InTypeInt16 => Some(PropSer(PropHandler::ArrayInt16)),
                    TdhInType::InTypeUInt16 => Some(PropSer(PropHandler::ArrayUInt16)),
                    TdhInType::InTypeInt32 => Some(PropSer(PropHandler::ArrayInt32)),
                    TdhInType::InTypeUInt32 => Some(PropSer(PropHandler::ArrayUInt32)),
                    TdhInType::InTypeInt64 => Some(PropSer(PropHandler::ArrayInt64)),
                    TdhInType::InTypeUInt64 => Some(PropSer(PropHandler::ArrayUInt64)),
                    TdhInType::InTypePointer => Some(PropSer(PropHandler::ArrayPointer)),
                    _ => None, // TODO
                }
            }
        }
    }
}

impl PropSerable for Property {
    fn get_parser(&self) -> Option<PropSer> {
        self.info.get_parser()
    }
}
