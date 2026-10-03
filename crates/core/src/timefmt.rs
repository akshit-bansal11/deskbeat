//! A small strftime subset for the clock widget.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    pub year: u32,
    /// 1 to 12.
    pub month: u32,
    pub day: u32,
    /// 0 is Sunday.
    pub weekday: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

const DAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Formats `t` with these specifiers, leaving anything else as written:
///
/// `%H` `%k` hour 00-23 / 0-23 · `%I` `%l` hour 01-12 / 1-12 · `%M` minute ·
/// `%S` second · `%p` AM/PM · `%A` `%a` weekday · `%B` `%b` month name ·
/// `%d` `%e` day 01-31 / 1-31 · `%m` month 01-12 · `%Y` `%y` year · `%%`.
pub fn format(fmt: &str, t: &LocalTime) -> String {
    let day_name = DAYS[t.weekday as usize % 7];
    let month_name = MONTHS[(t.month as usize).saturating_sub(1) % 12];
    let hour12 = match t.hour % 12 {
        0 => 12,
        h => h,
    };

    let mut out = String::with_capacity(fmt.len() + 8);
    let mut chars = fmt.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('H') => out += &format!("{:02}", t.hour),
            Some('k') => out += &t.hour.to_string(),
            Some('I') => out += &format!("{hour12:02}"),
            Some('l') => out += &hour12.to_string(),
            Some('M') => out += &format!("{:02}", t.minute),
            Some('S') => out += &format!("{:02}", t.second),
            Some('p') => out += if t.hour < 12 { "AM" } else { "PM" },
            Some('A') => out += day_name,
            Some('a') => out += &day_name[..3],
            Some('B') => out += month_name,
            Some('b') => out += &month_name[..3],
            Some('d') => out += &format!("{:02}", t.day),
            Some('e') => out += &t.day.to_string(),
            Some('m') => out += &format!("{:02}", t.month),
            Some('Y') => out += &t.year.to_string(),
            Some('y') => out += &format!("{:02}", t.year % 100),
            Some('%') => out.push('%'),
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

/// Whether a format shows seconds, and so needs a redraw every second
/// rather than every minute.
pub fn shows_seconds(fmt: &str) -> bool {
    fmt.contains("%S")
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: LocalTime = LocalTime {
        year: 2026,
        month: 10,
        day: 3,
        weekday: 6,
        hour: 21,
        minute: 7,
        second: 9,
    };

    #[test]
    fn formats_time_in_24_and_12_hour_forms() {
        assert_eq!(format("%H:%M:%S", &T), "21:07:09");
        assert_eq!(format("%I:%M %p", &T), "09:07 PM");
        assert_eq!(format("%l:%M", &T), "9:07");
    }

    #[test]
    fn midnight_and_noon_read_as_twelve() {
        let midnight = LocalTime { hour: 0, ..T };
        let noon = LocalTime { hour: 12, ..T };
        assert_eq!(format("%I %p", &midnight), "12 AM");
        assert_eq!(format("%l %p / %k", &noon), "12 PM / 12");
    }

    #[test]
    fn formats_dates() {
        assert_eq!(format("%A, %e %B %Y", &T), "Saturday, 3 October 2026");
        assert_eq!(format("%a %d/%m/%y", &T), "Sat 03/10/26");
        assert_eq!(format("%b", &T), "Oct");
    }

    #[test]
    fn leaves_unknown_specifiers_and_stray_percents_alone() {
        assert_eq!(format("100%% %Q %", &T), "100% %Q %");
    }

    #[test]
    fn out_of_range_fields_do_not_panic() {
        let odd = LocalTime {
            month: 0,
            weekday: 9,
            ..T
        };
        assert_eq!(format("%A %B", &odd), "Tuesday January");
    }

    #[test]
    fn detects_a_seconds_field() {
        assert!(shows_seconds("%H:%M:%S"));
        assert!(!shows_seconds("%H:%M"));
    }
}
