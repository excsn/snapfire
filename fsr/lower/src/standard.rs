//! The members JavaScript's standard library gives each kind of value the build tracks, read from node v26.8.1's prototypes.

use crate::Kind;

/// The methods and accessors on `kind`'s prototype. Empty for a kind with no prototype of its own.
pub(crate) fn members(kind: Kind) -> &'static [&'static str] {
  match kind {
    Kind::Map => &["clear", "delete", "entries", "forEach", "get", "getOrInsert", "getOrInsertComputed", "has", "keys", "set", "size", "values"],
    Kind::Set => &["add", "clear", "delete", "difference", "entries", "forEach", "has", "intersection", "isDisjointFrom", "isSubsetOf", "isSupersetOf", "keys", "size", "symmetricDifference", "union", "values"],
    Kind::WeakMap => &["delete", "get", "getOrInsert", "getOrInsertComputed", "has", "set"],
    Kind::WeakSet => &["add", "delete", "has"],
    Kind::WeakRef => &["deref"],
    Kind::Registry => &["register", "unregister"],
    Kind::Date => &["getDate", "getDay", "getFullYear", "getHours", "getMilliseconds", "getMinutes", "getMonth", "getSeconds", "getTime", "getTimezoneOffset", "getUTCDate", "getUTCDay", "getUTCFullYear", "getUTCHours", "getUTCMilliseconds", "getUTCMinutes", "getUTCMonth", "getUTCSeconds", "getYear", "setDate", "setFullYear", "setHours", "setMilliseconds", "setMinutes", "setMonth", "setSeconds", "setTime", "setUTCDate", "setUTCFullYear", "setUTCHours", "setUTCMilliseconds", "setUTCMinutes", "setUTCMonth", "setUTCSeconds", "setYear", "toDateString", "toGMTString", "toISOString", "toJSON", "toLocaleDateString", "toLocaleString", "toLocaleTimeString", "toString", "toTemporalInstant", "toTimeString", "toUTCString", "valueOf"],
    Kind::Query => &["append", "delete", "entries", "forEach", "get", "getAll", "has", "keys", "set", "size", "sort", "toString", "values"],
    Kind::Url => &["hash", "host", "hostname", "href", "origin", "password", "pathname", "port", "protocol", "search", "searchParams", "toJSON", "toString", "username"],
    Kind::Response => &["arrayBuffer", "blob", "body", "bodyUsed", "bytes", "clone", "formData", "headers", "json", "ok", "redirected", "status", "statusText", "text", "textStream", "type", "url"],
    Kind::Buffer => &["byteLength", "detached", "grow", "growable", "maxByteLength", "resizable", "resize", "slice", "transfer", "transferToFixedLength"],
    Kind::View => &["buffer", "byteLength", "byteOffset", "getBigInt64", "getBigUint64", "getFloat16", "getFloat32", "getFloat64", "getInt16", "getInt32", "getInt8", "getUint16", "getUint32", "getUint8", "setBigInt64", "setBigUint64", "setFloat16", "setFloat32", "setFloat64", "setInt16", "setInt32", "setInt8", "setUint16", "setUint32", "setUint8"],
    Kind::Stack => &["adopt", "defer", "dispose", "disposeAsync", "disposed", "move", "use"],
    Kind::Locale => &["baseName", "calendar", "caseFirst", "collation", "firstDayOfWeek", "getCalendars", "getCollations", "getHourCycles", "getNumberingSystems", "getTextInfo", "getTimeZones", "getWeekInfo", "hourCycle", "language", "maximize", "minimize", "numberingSystem", "numeric", "region", "script", "toString"],
    Kind::BigInt => &["toLocaleString", "toString", "valueOf"],
    Kind::Temporal => &["abs", "add", "blank", "calendarId", "day", "dayOfWeek", "dayOfYear", "days", "daysInMonth", "daysInWeek", "daysInYear", "epochMilliseconds", "epochNanoseconds", "equals", "era", "eraYear", "getTimeZoneTransition", "hour", "hours", "hoursInDay", "inLeapYear", "microsecond", "microseconds", "millisecond", "milliseconds", "minute", "minutes", "month", "monthCode", "months", "monthsInYear", "nanosecond", "nanoseconds", "negated", "offset", "offsetNanoseconds", "round", "second", "seconds", "sign", "since", "startOfDay", "subtract", "timeZoneId", "toInstant", "toJSON", "toLocaleString", "toPlainDate", "toPlainDateTime", "toPlainMonthDay", "toPlainTime", "toPlainYearMonth", "toString", "toZonedDateTime", "toZonedDateTimeISO", "total", "until", "valueOf", "weekOfYear", "weeks", "with", "withCalendar", "withPlainTime", "withTimeZone", "year", "yearOfWeek", "years"],
    Kind::Typed => &["BYTES_PER_ELEMENT", "at", "buffer", "byteLength", "byteOffset", "copyWithin", "entries", "every", "fill", "filter", "find", "findIndex", "findLast", "findLastIndex", "forEach", "includes", "indexOf", "join", "keys", "lastIndexOf", "length", "map", "reduce", "reduceRight", "reverse", "set", "setFromBase64", "setFromHex", "slice", "some", "sort", "subarray", "toBase64", "toHex", "toLocaleString", "toReversed", "toSorted", "toString", "values", "with"],
    Kind::Iterator => &["drop", "every", "filter", "find", "flatMap", "forEach", "map", "reduce", "some", "take", "toArray"],
    Kind::Error => &["message", "name", "toString"],
    Kind::RegExp => &["compile", "dotAll", "exec", "flags", "global", "hasIndices", "ignoreCase", "multiline", "source", "sticky", "test", "toString", "unicode", "unicodeSets"],
    Kind::Request => &["arrayBuffer", "attribute", "blob", "body", "bodyUsed", "bytes", "cache", "clone", "credentials", "destination", "duplex", "formData", "headers", "integrity", "isHistoryNavigation", "isReloadNavigation", "json", "keepalive", "method", "mode", "redirect", "referrer", "referrerPolicy", "signal", "text", "textStream", "url"],
    Kind::Symbol => &["description", "toString", "valueOf"],
    Kind::Browser | Kind::Scope | Kind::Wrapper | Kind::Proxy => &[],
  }
}

/// The members a value of `kind` holds as its own fields, which a read lowers to.
fn held(kind: Kind) -> &'static [&'static str] {
  match kind {
    Kind::RegExp => &["source", "flags", "global", "ignoreCase", "multiline", "sticky", "unicode", "unicodeSets", "dotAll", "hasIndices"],
    Kind::Locale => &["language", "script", "region", "baseName", "calendar", "caseFirst", "collation", "firstDayOfWeek", "hourCycle", "numeric", "numberingSystem"],
    Kind::Error => &["name", "message"],
    Kind::Stack => &["disposed"],
    _ => &[],
  }
}

/// Whether a read of `name` on a value of `kind` that no arm lowers is refused: a member of the kind's prototype that the value does not hold as a field.
pub(crate) fn unread(kind: Kind, name: &str) -> bool {
  members(kind).contains(&name) && !held(kind).contains(&name)
}

/// Whether a call of a method of a value of `kind` that no arm lowers is refused, rather than read as a call of a function held in a field.
pub(crate) fn uncalled(kind: Kind) -> bool {
  !members(kind).is_empty()
}
