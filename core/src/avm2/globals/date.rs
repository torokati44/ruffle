//! `Date` class

use crate::avm2::Error;
use crate::avm2::activation::Activation;
use crate::avm2::function::FunctionArgs;
use crate::avm2::object::{DateObject, clip_time};
use crate::avm2::parameters::ParametersExt;
use crate::avm2::value::Value;
use crate::string::{AvmString, WStr, utils as string_utils};
use num_traits::ToPrimitive;
use ruffle_macros::istr;

pub use crate::avm2::object::date_allocator;

const MS_PER_SECOND: i64 = 1000;
const MS_PER_MINUTE: i64 = 60 * MS_PER_SECOND;
const MS_PER_HOUR: i64 = 60 * MS_PER_MINUTE;
const MS_PER_DAY: i64 = 24 * MS_PER_HOUR;

const DAY_NAMES: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTH_NAMES: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// ECMA-262 Day - Get days since epoch.
fn day(time: i64) -> i64 {
    time.div_euclid(MS_PER_DAY)
}

/// ECMA-262 DayFromYear and the days before `month` (0-11) in that year -
/// Return days passed since epoch to the 1st of `month` in `year`.
///
/// This is Howard Hinnant's `days_from_civil` algorithm,
/// for the proleptic Gregorian calendar.
fn day_from_month(year: i128, month: i128) -> i128 {
    // Count years from March, so that the leap day is at the end of the year.
    let year = if month < 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let day_of_year = (153 * ((month + 10) % 12) + 2) / 5;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// ECMA-262 YearFromTime, MonthFromTime and DateFromTime -
/// Get the year, month (0-11) and date (1-31) of `day` days since epoch.
///
/// This is Howard Hinnant's `civil_from_days` algorithm,
/// for the proleptic Gregorian calendar.
fn year_month_date_from_day(day: i64) -> (i64, i64, i64) {
    let day = day + 719_468;
    let era = day.div_euclid(146_097);
    let day_of_era = day.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    // Months are counted from March, so that the leap day is at the end of the year.
    let shifted_month = (5 * day_of_year + 2) / 153;
    let date = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = (shifted_month + 2) % 12;
    let year = year_of_era + era * 400;
    (if month < 2 { year + 1 } else { year }, month, date)
}

/// ECMA-262 WeekDay - Get days within week (0-6), where 0 is Sunday.
fn week_day(time: i64) -> i64 {
    (day(time) + 4).rem_euclid(7)
}

/// ECMA-262 HourFromTime - Get hours (0-23).
fn hour_from_time(time: i64) -> i64 {
    time.div_euclid(MS_PER_HOUR).rem_euclid(24)
}

/// ECMA-262 MinFromTime - Get minutes (0-59).
fn min_from_time(time: i64) -> i64 {
    time.div_euclid(MS_PER_MINUTE).rem_euclid(60)
}

/// ECMA-262 SecFromTime - Get seconds (0-59).
fn sec_from_time(time: i64) -> i64 {
    time.div_euclid(MS_PER_SECOND).rem_euclid(60)
}

/// ECMA-262 msFromTime - Get milliseconds (0-999).
fn ms_from_time(time: i64) -> i64 {
    time.rem_euclid(MS_PER_SECOND)
}

/// ECMA-262 MakeTime
///
/// This can't overflow, as all the arguments fit in an `i64`.
fn make_time(hour: i64, minute: i64, second: i64, millisecond: i64) -> i128 {
    i128::from(hour) * i128::from(MS_PER_HOUR)
        + i128::from(minute) * i128::from(MS_PER_MINUTE)
        + i128::from(second) * i128::from(MS_PER_SECOND)
        + i128::from(millisecond)
}

/// ECMA-262 MakeDay - `month` is 0-based, and may be out of range.
///
/// This can't overflow, as all the arguments fit in an `i64`.
fn make_day(year: i64, month: i64, date: i64) -> i128 {
    let year = i128::from(year) + i128::from(month.div_euclid(12));
    let month = i128::from(month.rem_euclid(12));
    day_from_month(year, month) + i128::from(date) - 1
}

/// ECMA-262 MakeDate - Create from days since epoch and milliseconds within day.
fn make_date(day: i128, time: i128) -> i128 {
    day * i128::from(MS_PER_DAY) + time
}

/// Returns midnight on January 1st of year 0, in a timezone that is
/// `timezone` milliseconds ahead of UTC.
fn year_zero(timezone: i64) -> i64 {
    day_from_month(0, 0) as i64 * MS_PER_DAY - timezone
}

/// A date and time broken down into its components, in some timezone.
#[derive(Clone, Copy)]
struct DateFields {
    year: i64,
    /// 0-11
    month: i64,
    /// 1-31
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
    millisecond: i64,
    /// 0-6, where 0 is Sunday.
    weekday: i64,
    /// Milliseconds ahead of UTC.
    timezone: i64,
}

impl DateFields {
    /// Breaks down `time` (milliseconds since the epoch) into its components
    /// in a timezone that is `timezone` milliseconds ahead of UTC.
    fn new(time: i64, timezone: i64) -> Self {
        // ECMA-262 LocalTime - Convert from UTC to the timezone.
        let time = time.saturating_add(timezone);
        let (year, month, date) = year_month_date_from_day(day(time));
        Self {
            year,
            month,
            day: date,
            hour: hour_from_time(time),
            minute: min_from_time(time),
            second: sec_from_time(time),
            millisecond: ms_from_time(time),
            weekday: week_day(time),
            timezone,
        }
    }

    fn day_name(&self) -> &'static str {
        DAY_NAMES[self.weekday as usize]
    }

    fn month_name(&self) -> &'static str {
        MONTH_NAMES[self.month as usize]
    }

    /// The year, without padding. Years outside of 0-9999 get an explicit sign.
    fn year_string(&self) -> String {
        if (0..10_000).contains(&self.year) {
            self.year.to_string()
        } else {
            format!("{:+}", self.year)
        }
    }

    /// The time of day as `HH:MM:SS`.
    fn time_string(&self) -> String {
        format!("{:02}:{:02}:{:02}", self.hour, self.minute, self.second)
    }

    /// The timezone as `+HHMM`, rounded to the nearest minute.
    fn timezone_string(&self) -> String {
        let sign = if self.timezone < 0 { '-' } else { '+' };
        let minutes = (self.timezone.abs() / MS_PER_SECOND + 30) / 60;
        format!("{sign}{:02}{:02}", minutes / 60, minutes % 60)
    }

    fn am_pm(&self) -> &'static str {
        if self.hour < 12 { "AM" } else { "PM" }
    }
}

/// ECMA-262 LocalTZA - Get the local timezone, in milliseconds ahead of UTC.
fn local_timezone(activation: &Activation<'_, '_>) -> i64 {
    i64::from(activation.context.locale.get_timezone().seconds()) * MS_PER_SECOND
}

/// Breaks down the date stored in `this` in the local timezone.
fn local_fields(activation: &Activation<'_, '_>, this: DateObject<'_>) -> Option<DateFields> {
    let timezone = local_timezone(activation);
    this.time().map(|time| DateFields::new(time, timezone))
}

/// Breaks down the date stored in `this` in UTC.
fn utc_fields(this: DateObject<'_>) -> Option<DateFields> {
    this.time().map(|time| DateFields::new(time, 0))
}

/// Formats `date` with `format`, or returns "Invalid Date" if there's no date.
fn format_date<'gc>(
    activation: &mut Activation<'_, 'gc>,
    date: Option<DateFields>,
    format: impl FnOnce(&DateFields) -> String,
) -> Value<'gc> {
    if let Some(date) = date {
        AvmString::new_utf8(activation.gc(), format(&date)).into()
    } else {
        istr!("Invalid Date").into()
    }
}

struct DateAdjustment<'builder, 'activation_a: 'builder, 'gc: 'activation_a> {
    activation: &'builder mut Activation<'activation_a, 'gc>,
    /// Milliseconds ahead of UTC.
    timezone: i64,
    year: Option<Option<f64>>,
    month: Option<Option<f64>>,
    day: Option<Option<f64>>,
    hour: Option<Option<f64>>,
    minute: Option<Option<f64>>,
    second: Option<Option<f64>>,
    millisecond: Option<Option<f64>>,
}

impl<'builder, 'activation_a, 'gc> DateAdjustment<'builder, 'activation_a, 'gc> {
    fn new(activation: &'builder mut Activation<'activation_a, 'gc>, timezone: i64) -> Self {
        Self {
            activation,
            timezone,
            year: None,
            month: None,
            day: None,
            hour: None,
            minute: None,
            second: None,
            millisecond: None,
        }
    }

    fn map_year(&mut self, data_fn: impl Fn(f64) -> f64) -> &mut Self {
        if let Some(year) = self.year.flatten() {
            self.year = Some(Some(data_fn(year)));
        }
        self
    }

    fn year(&mut self, value: Option<Value<'gc>>) -> Result<&mut Self, Error<'gc>> {
        self.year = match value {
            Some(value) => Some(Some(value.coerce_to_number(self.activation)?)),
            None => None,
        };
        Ok(self)
    }

    fn month(&mut self, value: Option<Value<'gc>>) -> Result<&mut Self, Error<'gc>> {
        self.month = match value {
            Some(value) => Some(Some(value.coerce_to_number(self.activation)?)),
            None => None,
        };
        Ok(self)
    }

    fn day(&mut self, value: Option<Value<'gc>>) -> Result<&mut Self, Error<'gc>> {
        self.day = match value {
            Some(value) => Some(Some(value.coerce_to_number(self.activation)?)),
            None => None,
        };
        Ok(self)
    }

    fn hour(&mut self, value: Option<Value<'gc>>) -> Result<&mut Self, Error<'gc>> {
        self.hour = match value {
            Some(value) => Some(Some(value.coerce_to_number(self.activation)?)),
            None => None,
        };
        Ok(self)
    }

    fn minute(&mut self, value: Option<Value<'gc>>) -> Result<&mut Self, Error<'gc>> {
        self.minute = match value {
            Some(value) => Some(Some(value.coerce_to_number(self.activation)?)),
            None => None,
        };
        Ok(self)
    }

    fn second(&mut self, value: Option<Value<'gc>>) -> Result<&mut Self, Error<'gc>> {
        self.second = match value {
            Some(value) => Some(Some(value.coerce_to_number(self.activation)?)),
            None => None,
        };
        Ok(self)
    }

    fn millisecond(&mut self, value: Option<Value<'gc>>) -> Result<&mut Self, Error<'gc>> {
        self.millisecond = match value {
            Some(value) => Some(Some(value.coerce_to_number(self.activation)?)),
            None => None,
        };
        Ok(self)
    }

    fn check_value(
        &self,
        specified: Option<Option<f64>>,
        current: impl ToPrimitive,
    ) -> Option<i64> {
        match specified {
            Some(Some(value)) if value.is_finite() => Some(value as i64),
            Some(_) => None,
            None => current.to_i64(),
        }
    }

    /// Applies the adjustments to `current`, returning the resulting time
    /// in milliseconds since the epoch.
    fn calculate(&mut self, current: DateFields) -> Option<i64> {
        let year = self.check_value(self.year, current.year)?;
        let month = self.check_value(self.month, current.month)?;
        let date = self.check_value(self.day, current.day)?;
        let hour = self.check_value(self.hour, current.hour)?;
        let minute = self.check_value(self.minute, current.minute)?;
        let second = self.check_value(self.second, current.second)?;
        let millisecond = self.check_value(self.millisecond, current.millisecond)?;

        let time = make_date(
            make_day(year, month, date),
            make_time(hour, minute, second, millisecond),
        );
        // ECMA-262 UTC - Convert from the timezone to UTC.
        let time = time - i128::from(current.timezone);
        clip_time(i64::try_from(time).ok()?)
    }

    fn apply(&mut self, object: DateObject<'gc>) -> f64 {
        let timezone = self.timezone;
        let time = object
            .time()
            .and_then(|current| self.calculate(DateFields::new(current, timezone)));
        object.set_time(time);
        if let Some(time) = time {
            time as f64
        } else {
            f64::NAN
        }
    }
}

pub fn init_custom_prototype<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();
    let this = this.as_class_object().unwrap();

    let prototype_date_object = DateObject::for_prototype(activation.context, this);

    this.link_prototype(activation.context, prototype_date_object);

    Ok(Value::Undefined)
}

/// Implements `Date`'s instance constructor.
pub fn init<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    let timestamp = args.get_optional(0).unwrap_or(Value::Undefined);
    if !matches!(timestamp, Value::Undefined) {
        if args.len() > 1 {
            let timezone = local_timezone(activation);

            // We need a starting value to adjust from.
            this.set_time(Some(year_zero(timezone)));

            DateAdjustment::new(activation, timezone)
                .year(args.get_optional(0))?
                .month(args.get_optional(1))?
                .day(args.get_optional(2))?
                .hour(args.get_optional(3))?
                .minute(args.get_optional(4))?
                .second(args.get_optional(5))?
                .millisecond(args.get_optional(6))?
                .map_year(|year| if year < 100.0 { year + 1900.0 } else { year })
                .apply(this);
        } else {
            let timestamp = if let Value::String(date_str) = timestamp {
                parse_full_date(activation, date_str).unwrap_or(f64::NAN)
            } else {
                timestamp.coerce_to_number(activation)?
            };
            if timestamp.is_finite() {
                this.set_time(clip_time(timestamp as i64))
            }
        }
    } else {
        let now = activation.context.locale.get_current_date_time();
        this.set_time(Some(now.as_millisecond()))
    }

    Ok(Value::Undefined)
}

pub fn call_handler<'gc>(
    activation: &mut Activation<'_, 'gc>,
    _this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    activation.avm2().classes().date.construct(activation, &[])
}

/// Implements `getTime` method.
pub fn get_time<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    if let Some(time) = this.time() {
        Ok((time as f64).into())
    } else {
        Ok(f64::NAN.into())
    }
}

/// Implements `setTime` method.
pub fn _set_time<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    let new_time = args.get_f64(0);

    if new_time.is_finite()
        && let Some(time) = clip_time(new_time as i64)
    {
        this.set_time(Some(time));
        Ok((time as f64).into())
    } else {
        this.set_time(None);
        Ok(f64::NAN.into())
    }
}

/// Implements the `getMilliseconds` method.
pub fn get_milliseconds<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    if let Some(date) = local_fields(activation, this) {
        Ok((date.millisecond as f64).into())
    } else {
        Ok(f64::NAN.into())
    }
}

/// Implements the `setMilliseconds` method.
pub fn _set_milliseconds<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    let timezone = local_timezone(activation);
    let timestamp = DateAdjustment::new(activation, timezone)
        .millisecond(args.get_optional(0))?
        .apply(this);
    Ok(timestamp.into())
}

/// Implements the `getSeconds` method.
pub fn get_seconds<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    if let Some(date) = local_fields(activation, this) {
        Ok((date.second as f64).into())
    } else {
        Ok(f64::NAN.into())
    }
}

/// Implements `setSeconds` method.
pub fn _set_seconds<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    let timezone = local_timezone(activation);
    let timestamp = DateAdjustment::new(activation, timezone)
        .second(args.get_optional(0))?
        .millisecond(args.get_optional(1))?
        .apply(this);
    Ok(timestamp.into())
}

/// Implements `getMinutes` method.
pub fn get_minutes<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    if let Some(date) = local_fields(activation, this) {
        Ok((date.minute as f64).into())
    } else {
        Ok(f64::NAN.into())
    }
}

/// Implements the `setMinutes` method.
pub fn _set_minutes<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    let timezone = local_timezone(activation);
    let timestamp = DateAdjustment::new(activation, timezone)
        .minute(args.get_optional(0))?
        .second(args.get_optional(1))?
        .millisecond(args.get_optional(2))?
        .apply(this);
    Ok(timestamp.into())
}

/// Implements the `getHours` method.
pub fn get_hours<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    if let Some(date) = local_fields(activation, this) {
        Ok((date.hour as f64).into())
    } else {
        Ok(f64::NAN.into())
    }
}

/// Implements `setHours` method.
pub fn _set_hours<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    let timezone = local_timezone(activation);
    let timestamp = DateAdjustment::new(activation, timezone)
        .hour(args.get_optional(0))?
        .minute(args.get_optional(1))?
        .second(args.get_optional(2))?
        .millisecond(args.get_optional(3))?
        .apply(this);
    Ok(timestamp.into())
}

/// Implements `getDate` method.
pub fn get_date<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    if let Some(date) = local_fields(activation, this) {
        Ok((date.day as f64).into())
    } else {
        Ok(f64::NAN.into())
    }
}

/// Implements `setDate` method.
pub fn _set_date<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    let timezone = local_timezone(activation);
    let timestamp = DateAdjustment::new(activation, timezone)
        .day(args.get_optional(0))?
        .apply(this);
    Ok(timestamp.into())
}

/// Implements the `getMonth` method.
pub fn get_month<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    if let Some(date) = local_fields(activation, this) {
        Ok((date.month as f64).into())
    } else {
        Ok(f64::NAN.into())
    }
}

/// Implements the `setMonth` method.
pub fn _set_month<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    let timezone = local_timezone(activation);
    let timestamp = DateAdjustment::new(activation, timezone)
        .month(args.get_optional(0))?
        .day(args.get_optional(1))?
        .apply(this);
    Ok(timestamp.into())
}

/// Implements the `getFullYear` method.
pub fn get_full_year<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    if let Some(date) = local_fields(activation, this) {
        Ok((date.year as f64).into())
    } else {
        Ok(f64::NAN.into())
    }
}

/// Implements the `setFullYear` method.
pub fn _set_full_year<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    let timezone = local_timezone(activation);
    if this.time().is_none() {
        this.set_time(Some(year_zero(timezone)));
    }
    let timestamp = DateAdjustment::new(activation, timezone)
        .year(args.get_optional(0))?
        .month(args.get_optional(1))?
        .day(args.get_optional(2))?
        .apply(this);
    Ok(timestamp.into())
}

/// Implements the `getDay` method.
pub fn get_day<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    if let Some(date) = local_fields(activation, this) {
        Ok((date.weekday as f64).into())
    } else {
        Ok(f64::NAN.into())
    }
}

/// Implements the `getUTCMilliseconds` method.
pub fn get_utc_milliseconds<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    if let Some(date) = utc_fields(this) {
        Ok((date.millisecond as f64).into())
    } else {
        Ok(f64::NAN.into())
    }
}

/// Implements the `setUTCMilliseconds` method.
pub fn _set_utc_milliseconds<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    let timestamp = DateAdjustment::new(activation, 0)
        .millisecond(args.get_optional(0))?
        .apply(this);
    Ok(timestamp.into())
}

/// Implements the `getUTCSeconds` method.
pub fn get_utc_seconds<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    if let Some(date) = utc_fields(this) {
        Ok((date.second as f64).into())
    } else {
        Ok(f64::NAN.into())
    }
}

/// Implements the `setUTCSeconds` method.
pub fn _set_utc_seconds<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    let timestamp = DateAdjustment::new(activation, 0)
        .second(args.get_optional(0))?
        .millisecond(args.get_optional(1))?
        .apply(this);
    Ok(timestamp.into())
}

/// Implements the `getUTCMinutes` method.
pub fn get_utc_minutes<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    if let Some(date) = utc_fields(this) {
        Ok((date.minute as f64).into())
    } else {
        Ok(f64::NAN.into())
    }
}

/// Implements the `setUTCMinutes` method.
pub fn _set_utc_minutes<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    let timestamp = DateAdjustment::new(activation, 0)
        .minute(args.get_optional(0))?
        .second(args.get_optional(1))?
        .millisecond(args.get_optional(2))?
        .apply(this);
    Ok(timestamp.into())
}

/// Implements the `getUTCHours` method.
pub fn get_utc_hours<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    if let Some(date) = utc_fields(this) {
        Ok((date.hour as f64).into())
    } else {
        Ok(f64::NAN.into())
    }
}

/// Implements the `setUTCHours` method.
pub fn _set_utc_hours<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    let timestamp = DateAdjustment::new(activation, 0)
        .hour(args.get_optional(0))?
        .minute(args.get_optional(1))?
        .second(args.get_optional(2))?
        .millisecond(args.get_optional(3))?
        .apply(this);
    Ok(timestamp.into())
}

/// Implements the `getUTCDate` method.
pub fn get_utc_date<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    if let Some(date) = utc_fields(this) {
        Ok((date.day as f64).into())
    } else {
        Ok(f64::NAN.into())
    }
}

/// Implements the `setUTCDate` method.
pub fn _set_utc_date<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    let timestamp = DateAdjustment::new(activation, 0)
        .day(args.get_optional(0))?
        .apply(this);
    Ok(timestamp.into())
}

/// Implements the `getUTCMonth` method.
pub fn get_utc_month<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    if let Some(date) = utc_fields(this) {
        Ok((date.month as f64).into())
    } else {
        Ok(f64::NAN.into())
    }
}

/// Implements the `setUTCMonth` method.
pub fn _set_utc_month<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    let timestamp = DateAdjustment::new(activation, 0)
        .month(args.get_optional(0))?
        .day(args.get_optional(1))?
        .apply(this);
    Ok(timestamp.into())
}

/// Implements the `getUTCFullYear` method.
pub fn get_utc_full_year<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    if let Some(date) = utc_fields(this) {
        Ok((date.year as f64).into())
    } else {
        Ok(f64::NAN.into())
    }
}

/// Implements the `setUTCFullYear` method.
pub fn _set_utc_full_year<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    if this.time().is_none() {
        this.set_time(Some(year_zero(0)));
    }
    let timestamp = DateAdjustment::new(activation, 0)
        .year(args.get_optional(0))?
        .month(args.get_optional(1))?
        .day(args.get_optional(2))?
        .apply(this);
    Ok(timestamp.into())
}

/// Implements the `getUTCDay` method.
pub fn get_utc_day<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    if let Some(date) = utc_fields(this) {
        Ok((date.weekday as f64).into())
    } else {
        Ok(f64::NAN.into())
    }
}

/// Implements the `getTimezoneOffset` method.
pub fn get_timezone_offset<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    if this.time().is_some() {
        let offset = -local_timezone(activation) as f64;
        Ok((offset / MS_PER_MINUTE as f64).into())
    } else {
        Ok(f64::NAN.into())
    }
}

/// Implements the `UTC` class method.
pub fn utc<'gc>(
    activation: &mut Activation<'_, 'gc>,
    _this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let year = args.get_value(0);
    let month = args.get_value(1);
    let day = args.get_value(2);
    let hour = args.get_value(3);
    let minute = args.get_value(4);
    let second = args.get_value(5);
    let millisecond = args.get_value(6);

    let date = DateAdjustment::new(activation, 0)
        .year(Some(year))?
        .month(Some(month))?
        .day(Some(day))?
        .hour(Some(hour))?
        .minute(Some(minute))?
        .second(Some(second))?
        .millisecond(Some(millisecond))?
        .map_year(|year| if year < 100.0 { year + 1900.0 } else { year })
        .calculate(DateFields::new(year_zero(0), 0));
    let millis = if let Some(date) = date {
        date as f64
    } else {
        f64::NAN
    };

    Ok(millis.into())
}

/// Implements the `toString` method.
pub fn to_string<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    let date = local_fields(activation, this);
    Ok(format_date(activation, date, |date| {
        format!(
            "{} {} {} {} GMT{} {}",
            date.day_name(),
            date.month_name(),
            date.day,
            date.time_string(),
            date.timezone_string(),
            date.year_string()
        )
    }))
}

/// Implements the `toUTCString` method.
pub fn to_utc_string<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    Ok(format_date(activation, utc_fields(this), |date| {
        format!(
            "{} {} {} {} {} UTC",
            date.day_name(),
            date.month_name(),
            date.day,
            date.time_string(),
            date.year_string()
        )
    }))
}

/// Implements the `toLocaleString` method.
pub fn to_locale_string<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    let date = local_fields(activation, this);
    Ok(format_date(activation, date, |date| {
        format!(
            "{} {} {} {} {} {}",
            date.day_name(),
            date.month_name(),
            date.day,
            date.year_string(),
            date.time_string(),
            date.am_pm()
        )
    }))
}

/// Implements the `toTimeString` method.
pub fn to_time_string<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    let date = local_fields(activation, this);
    Ok(format_date(activation, date, |date| {
        format!("{} GMT{}", date.time_string(), date.timezone_string())
    }))
}

/// Implements the `toLocaleTimeString` method.
pub fn to_locale_time_string<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    let date = local_fields(activation, this);
    Ok(format_date(activation, date, |date| {
        format!("{} {}", date.time_string(), date.am_pm())
    }))
}

/// Implements the `toDateString` & `toLocaleDateString` method.
pub fn to_date_string<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Value<'gc>,
    _args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let this = this.as_object().unwrap();

    let this = this.as_date_object().unwrap();

    let date = local_fields(activation, this);
    Ok(format_date(activation, date, |date| {
        format!(
            "{} {} {} {}",
            date.day_name(),
            date.month_name(),
            date.day,
            date.year_string()
        )
    }))
}

/// Parse a date, in any of the three formats: YYYY/MM/DD, MM/DD/YYYY, Mon/DD/YYYY.
/// The output will always be: (year, month, day), or None if format is invalid.
fn parse_date(item: &WStr) -> Option<(u32, u32, u32)> {
    let mut iter = item.split(b'/');
    let first = iter.next()?;
    let parsed = if first.len() == 4 {
        // If the first item in this date is 4 characters long, we parse as YYYY/MM/DD
        let month = iter.next()?;
        if month.len() != 2 {
            return None;
        }
        let day = iter.next()?;
        if day.len() != 2 {
            return None;
        }
        (
            first.parse::<u32>().ok()?,
            month.parse::<u32>().ok()?.checked_sub(1)?,
            day.parse::<u32>().ok()?,
        )
    } else if first.len() == 2 {
        // If the first item in this date is 2 characters long, we parse as MM/DD/YYYY
        let day = iter.next()?;
        if day.len() != 2 {
            return None;
        }
        let year = iter.next()?;
        if year.len() != 4 {
            return None;
        }
        (
            year.parse::<u32>().ok()?,
            first.parse::<u32>().ok()?.checked_sub(1)?,
            day.parse::<u32>().ok()?,
        )
    } else if first.len() == 3 {
        // If the first item in this date is 3 characters long, we parse as Mon/DD/YYYY

        // First lets parse the Month
        let month = parse_mon(first)?;
        let day = iter.next()?;
        if day.len() != 2 {
            return None;
        }
        let year = iter.next()?;
        if year.len() != 4 {
            return None;
        }
        (
            year.parse::<u32>().ok()?,
            month as u32,
            day.parse::<u32>().ok()?,
        )
    } else {
        return None;
    };
    if iter.next().is_some() {
        // the iterator should have been empty
        return None;
    }
    Some(parsed)
}

/// Convert a month abbreviation to a number.
fn parse_mon(item: &WStr) -> Option<usize> {
    const MONTHS: [&[u8]; 12] = [
        b"Jan", b"Feb", b"Mar", b"Apr", b"May", b"Jun", b"Jul", b"Aug", b"Sep", b"Oct", b"Nov",
        b"Dec",
    ];
    MONTHS.iter().position(|&x| x == item)
}

/// Parses HH:MM:SS. The output is always (hours, minutes, seconds), or None if format was invalid.
fn parse_hms(item: &WStr) -> Option<(u32, u32, u32)> {
    let mut iter = item.split(b':');
    let hours = iter.next()?;
    if hours.len() != 2 {
        return None;
    }
    let minutes = iter.next()?;
    if minutes.len() != 2 {
        return None;
    }
    let seconds = iter.next()?;
    if seconds.len() != 2 {
        return None;
    }
    if iter.next().is_some() {
        // the iterator should have been empty
        return None;
    }
    Some((
        hours.parse::<u32>().ok()?,
        minutes.parse::<u32>().ok()?,
        seconds.parse::<u32>().ok()?,
    ))
}

pub fn parse_full_date<'gc>(
    activation: &mut Activation<'_, 'gc>,
    date: AvmString<'gc>,
) -> Option<f64> {
    const DAYS: [&[u8]; 7] = [b"Sun", b"Mon", b"Tue", b"Wed", b"Thu", b"Fri", b"Sat"];

    let timezone = local_timezone(activation);
    let mut final_time = DateAdjustment::new(activation, timezone);
    let mut new_timezone = None;
    // The Date parser is flash is super flexible, so we need to go through each item individually and parse it to match Flash.
    // NOTE: DateTime::parse_from_str is not flexible enough for this, so we need to parse manually.
    for item in date
        .split(string_utils::swf_is_whitespace)
        .filter(|s| !s.is_empty())
    {
        if let Some((year, month, day)) = parse_date(item) {
            // Parse YYYY/MM/DD, MM/DD/YYYY, Mon/DD/YYYY

            // First we check if the fields we are going to set have already been set, if they are, we return NaN.
            // The same logic applies for all other if/else branches.
            if final_time.year.is_some() || final_time.month.is_some() || final_time.day.is_some() {
                return None;
            }
            final_time.year = Some(Some(year as f64));
            final_time.month = Some(Some(month as f64));
            final_time.day = Some(Some(day as f64));
        } else if let Some((hours, minutes, seconds)) = parse_hms(item) {
            // Parse HH:MM:SS

            if final_time.hour.is_some()
                || final_time.minute.is_some()
                || final_time.second.is_some()
            {
                return None;
            }
            final_time.hour = Some(Some(hours as f64));
            final_time.minute = Some(Some(minutes as f64));
            final_time.second = Some(Some(seconds as f64));
        } else if DAYS.iter().any(|&d| d == item) {
            // Parse abbreviated weekname (Sun, Mon, etc...)
            // DO NOTHING
        } else if let Some(month) = parse_mon(item) {
            // Parse abbreviated month name (Jan, Feb, etc...)
            final_time.month = Some(Some(month as f64));
        } else if item.starts_with(WStr::from_units(b"GMT"))
            || item.starts_with(WStr::from_units(b"UTC"))
        {
            // Parse GMT-HHMM/GMT+HHMM or UTC-HHMM/UTC+HHMM
            // Also handle standalone GMT/UTC (means UTC+0000)

            if new_timezone.is_some() {
                return None;
            }

            if item == b"GMT" || item == b"UTC" {
                new_timezone = Some(0);
            } else if item.len() == 8 {
                let (other, tzn) = item.split_at(4);
                if tzn.len() != 4 {
                    return None;
                }
                let (hours, minutes) = tzn.split_at(2);
                let hours = hours.parse::<u32>().ok()?;
                let minutes = minutes.parse::<u32>().ok()?;
                let sign = other.at(3);
                // NOTE: In real flash, invalid (out of bounds) timezones were allowed, but for now we only accept offsets below 24 hours.
                // Since it is insanely rare to ever parse a date with an invalid timezone, for now we just return an error.
                let offset = i64::from(hours) * MS_PER_HOUR + i64::from(minutes) * MS_PER_MINUTE;
                if offset >= MS_PER_DAY {
                    return None;
                }
                new_timezone = Some(if sign == b'-' as u16 {
                    -offset
                } else if sign == b'+' as u16 {
                    offset
                } else {
                    return None;
                });
            } else {
                return None;
            }
        } else if let Ok(mut num) = item.parse::<u32>() {
            // Parse either a day or a year

            // If the number is greater than 70, lets parse as a year
            if num >= 70 {
                if final_time.year.is_some() {
                    return None;
                }
                // If the number is less than 100, we add 1900 to it.
                if num < 100 {
                    num += 1900;
                }
                final_time.year = Some(Some(num as f64));
            // Otherwise, lets parse as a day
            } else {
                if final_time.day.is_some() {
                    return None;
                }
                final_time.day = Some(Some(num as f64))
            }
        } else {
            return None;
        }
    }
    // It is required that year, month, and day all have data.
    if final_time.year.is_none() || final_time.month.is_none() || final_time.day.is_none() {
        return None;
    }
    let timezone = new_timezone.unwrap_or(timezone);
    if let Some(time) = final_time.calculate(DateFields::new(year_zero(timezone), timezone)) {
        Some(time as f64)
    } else {
        None
    }
}

/// Implements the `parse` class method.
pub fn parse<'gc>(
    activation: &mut Activation<'_, 'gc>,
    _this: Value<'gc>,
    args: FunctionArgs<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    let date_str = args.get_value(0).coerce_to_string(activation)?;

    Ok(parse_full_date(activation, date_str)
        .unwrap_or(f64::NAN)
        .into())
}
