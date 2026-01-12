//! CtApi constants

/// user error base
pub const ERROR_USER_DEFINED_BASE: u32 = 0x10000000;

/// range check the variable
pub const CT_SCALE_RANGE_CHECK: u32 = 0x00000001;
/// clamp variable at limits
pub const CT_SCALE_CLAMP_LIMIT: u32 = 0x00000002;
/// noise factor,on limits
pub const CT_SCALE_NOISE_FACTOR: u32 = 0x00000004;

/// don't scale the variable
pub const CT_FMT_NO_SCALE: u32 = 0x00000001;
/// don't apply format
pub const CT_FMT_NO_FORMAT: u32 = 0x00000002;
/// get last value of data
pub const CT_FMT_LAST: u32 = 0x00000004;
/// range check the variable
pub const CT_FMT_RANGE_CHECK: u32 = 0x00000008;

/// scroll to next record
pub const CT_FIND_SCROLL_NEXT: u32 = 0x00000001;
/// scroll to prev record
pub const CT_FIND_SCROLL_PREV: u32 = 0x00000002;
/// scroll to first record
pub const CT_FIND_SCROLL_FIRST: u32 = 0x00000003;
/// scroll to last record
pub const CT_FIND_SCROLL_LAST: u32 = 0x00000004;
/// scroll to absolute record
pub const CT_FIND_SCROLL_ABSOLUTE: u32 = 0x00000005;
/// scroll to relative record
pub const CT_FIND_SCROLL_RELATIVE: u32 = 0x00000006;

/// use encryption
pub const CT_OPEN_CRYPT: u32 = 0x00000001;
/// reconnect on failure
pub const CT_OPEN_RECONNECT: u32 = 0x00000002;
/// read only mode
pub const CT_OPEN_READ_ONLY: u32 = 0x00000004;
/// batch mode
pub const CT_OPEN_BATCH: u32 = 0x00000008;

/// list event mode
pub const CT_LIST_EVENT: u32 = 0x00000001;
/// list lightweight mode
pub const CT_LIST_LIGHTWEIGHT_MODE: u32 = 0x00000002;

/// get event for new tags
pub const CT_LIST_EVENT_NEW: u32 = 0x00000001;
/// get events for status change
pub const CT_LIST_EVENT_STATUS: u32 = 0x00000002;

/// value
pub const CT_LIST_VALUE: u32 = 0x00000001;
/// timestamp
pub const CT_LIST_TIMESTAMP: u32 = 0x00000002;
/// valueTimestamp
pub const CT_LIST_VALUE_TIMESTAMP: u32 = 0x00000003;
/// qualityTimestamp
pub const CT_LIST_QUALITY_TIMESTAMP: u32 = 0x00000004;
/// quality general
pub const CT_LIST_QUALITY_GENERAL: u32 = 0x00000005;
/// quality substatus
pub const CT_LIST_QUALITY_SUBSTATUS: u32 = 0x00000006;
/// quality limit
pub const CT_LIST_QUALITY_LIMIT: u32 = 0x00000007;
/// quality extended substatus
pub const CT_LIST_QUALITY_EXTENDED_SUBSTATUS: u32 = 0x00000008;
/// quality datasource error
pub const CT_LIST_QUALITY_DATASOURCE_ERROR: u32 = 0x00000009;
/// quality override
pub const CT_LIST_QUALITY_OVERRIDE: u32 = 0x0000000A;
/// quality control mode
pub const CT_LIST_QUALITY_CONTROL_MODE: u32 = 0x0000000B;

/// property name length
pub const PROPERTY_NAME_LEN: u32 = 256;
