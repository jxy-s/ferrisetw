//! ETW Event Schema and handler
//!
//! This module contains the means needed to interact with the Schema of an ETW event
use crate::native::etw_types::DecodingSource;
use crate::native::tdh::TraceEventInfo;
use crate::native::tdh_types::{Property, PropertyError};
use once_cell::sync::OnceCell;

/// A schema suitable for parsing a given kind of event.
///
/// It is usually retrieved from [`crate::schema_locator::SchemaLocator::event_schema`].
///
/// This structure is basically a wrapper over a [TraceEventInfo](https://docs.microsoft.com/en-us/windows/win32/api/tdh/ns-tdh-trace_event_info),
/// with a few info parsed (and cached) out of it
pub struct Schema {
    te_info: TraceEventInfo,
    cached_properties: OnceCell<Result<Vec<Property>, PropertyError>>,
}

impl Schema {
    pub(crate) fn new(te_info: TraceEventInfo) -> Self {
        Schema {
            te_info,
            cached_properties: OnceCell::new(),
        }
    }

    /// Use the `decoding_source` function to obtain the [DecodingSource] from the `TRACE_EVENT_INFO`
    ///
    /// This getter returns the DecodingSource from the event, this value identifies the source used
    /// parse the event data
    ///
    /// # Example
    /// ```
    /// # use ferrisetw::EventRecord;
    /// # use ferrisetw::schema_locator::SchemaLocator;

    /// let my_callback = |record: &EventRecord, schema_locator: &SchemaLocator| {
    ///     let schema = schema_locator.event_schema(record).unwrap();
    ///     let decoding_source = schema.decoding_source();
    /// };
    /// ```
    pub fn decoding_source(&self) -> DecodingSource {
        self.te_info.decoding_source()
    }

    /// Use the `provider_name` function to obtain the Provider name from the `TRACE_EVENT_INFO`
    ///
    /// # Example
    /// ```
    /// # use ferrisetw::EventRecord;
    /// # use ferrisetw::schema_locator::SchemaLocator;
    /// let my_callback = |record: &EventRecord, schema_locator: &SchemaLocator| {
    ///     let schema = schema_locator.event_schema(record).unwrap();
    ///     let provider_name = schema.provider_name();
    /// };
    /// ```
    /// [TraceEventInfo]: crate::native::tdh::TraceEventInfo
    pub fn provider_name(&self) -> String {
        self.te_info.provider_name()
    }

    /// Use the `task_name` function to obtain the Task name from the `TRACE_EVENT_INFO`
    ///
    /// See: [TaskType](https://docs.microsoft.com/en-us/windows/win32/wes/eventmanifestschema-tasktype-complextype)
    /// # Example
    /// ```
    /// # use ferrisetw::EventRecord;
    /// # use ferrisetw::schema_locator::SchemaLocator;
    /// let my_callback = |record: &EventRecord, schema_locator: &SchemaLocator| {
    ///     let schema = schema_locator.event_schema(record).unwrap();
    ///     let task_name = schema.task_name();
    /// };
    /// ```
    /// [TraceEventInfo]: crate::native::tdh::TraceEventInfo
    pub fn task_name(&self) -> String {
        self.te_info.task_name()
    }

    /// Use the `opcode_name` function to obtain the Opcode name from the `TRACE_EVENT_INFO`
    ///
    /// See: [OpcodeType](https://docs.microsoft.com/en-us/windows/win32/wes/eventmanifestschema-opcodetype-complextype)
    /// # Example
    /// ```
    /// # use ferrisetw::EventRecord;
    /// # use ferrisetw::schema_locator::SchemaLocator;
    /// let my_callback = |record: &EventRecord, schema_locator: &SchemaLocator| {
    ///     let schema = schema_locator.event_schema(record).unwrap();
    ///     let opcode_name = schema.opcode_name();
    /// };
    /// ```
    /// [TraceEventInfo]: crate::native::tdh::TraceEventInfo
    pub fn opcode_name(&self) -> String {
        self.te_info.opcode_name()
    }

    /// Parses the list of properties of the wrapped `TRACE_EVENT_INFO`
    ///
    /// This is parsed on first call, and cached for later use
    pub(crate) fn properties(&self) -> &[Property] {
        match self.try_properties() {
            Err(PropertyError::UnimplementedType(_)) => {
                log::error!("Unable to list properties: a type is not implemented");
                &[]
            }
            Ok(p) => p,
        }
    }

    pub(crate) fn try_properties(&self) -> Result<&[Property], PropertyError> {
        let cache = self.cached_properties.get_or_init(|| {
            let mut cache = Vec::new();
            for property in self.te_info.properties() {
                cache.push(property?)
            }
            Ok(cache)
        });

        match cache {
            Err(e) => Err(e.clone()),
            Ok(cache) => Ok(cache.as_slice()),
        }
    }
}

impl PartialEq for Schema {
    fn eq(&self, other: &Self) -> bool {
        self.te_info.event_id() == other.te_info.event_id()
            && self.te_info.provider_guid() == other.te_info.provider_guid()
            && self.te_info.event_version() == other.te_info.event_version()
    }
}

impl Eq for Schema {}
