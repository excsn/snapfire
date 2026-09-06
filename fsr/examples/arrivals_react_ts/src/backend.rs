use std::sync::Arc;
use std::time::{Duration, Instant};

use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_service::{LocalTransport, Transport};

/// One flight on the schedule. `at` is minutes past midnight, `delay` the
/// minutes it is running late and `moved` a gate change with the minute the
/// field announced it.
struct Scheduled {
  flight: &'static str,
  city: &'static str,
  at: i64,
  delay: i64,
  gate: &'static str,
  moved: Option<(i64, &'static str)>,
}

const ARRIVALS: &[Scheduled] = &[
  Scheduled { flight: "BA 118", city: "New York JFK", at: 440, delay: 0, gate: "A12", moved: None },
  Scheduled { flight: "LH 906", city: "Frankfurt", at: 465, delay: 0, gate: "B04", moved: None },
  Scheduled { flight: "AF 1680", city: "Paris CDG", at: 485, delay: 35, gate: "B11", moved: Some((452, "B14")) },
  Scheduled { flight: "KL 1007", city: "Amsterdam", at: 510, delay: 0, gate: "A03", moved: Some((498, "A07")) },
  Scheduled { flight: "SK 1512", city: "Copenhagen", at: 535, delay: 15, gate: "C05", moved: None },
  Scheduled { flight: "TP 1234", city: "Lisbon", at: 560, delay: 0, gate: "A09", moved: None },
];

const DEPARTURES: &[Scheduled] = &[
  Scheduled { flight: "KL 1008", city: "Amsterdam", at: 455, delay: 0, gate: "A07", moved: None },
  Scheduled { flight: "IB 3241", city: "Madrid", at: 480, delay: 0, gate: "C02", moved: None },
  Scheduled { flight: "AZ 205", city: "Rome", at: 500, delay: 25, gate: "B08", moved: Some((470, "B02")) },
  Scheduled { flight: "LX 318", city: "Zurich", at: 525, delay: 0, gate: "C11", moved: None },
  Scheduled { flight: "EI 521", city: "Dublin", at: 550, delay: 0, gate: "A02", moved: None },
];

const FIELDS: [&str; 4] = ["overcast", "clear", "light rain", "hazy"];

/// The field's morning: it opens at 07:00 and the schedule runs out before
/// 10:20, so the clock wraps there and the morning happens again. A board
/// that empties after one pass is a board nobody can watch.
const OPENS: i64 = 420;
const MORNING: i64 = 200;

/// The clock's reading brought back into the morning it repeats.
fn field_time(raw: i64) -> i64 {
  OPENS + (raw - OPENS).rem_euclid(MORNING)
}

/// The field's clock. `Running` accelerates: one real second is `speed`
/// simulated minutes, so a morning passes while you watch. `Frozen` is what a
/// test uses, since a board that moves is a board no assertion can hold.
#[derive(Clone)]
enum Clock {
  Running { from: Instant, start: i64, speed: f64 },
  Frozen(i64),
}

impl Clock {
  fn minutes(&self) -> i64 {
    match self {
      Clock::Running { from, start, speed } => start + (from.elapsed().as_secs_f64() * speed) as i64,
      Clock::Frozen(at) => *at,
    }
  }
}

/// `07:45` from minutes past midnight, wrapping at a day.
fn clock(minutes: i64) -> String {
  let minutes = minutes.rem_euclid(24 * 60);
  format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

fn flight(row: &Scheduled, now: i64, departing: bool) -> Value {
  let expected = row.at + row.delay;
  let (status, code) = if now >= expected {
    match departing {
      true => ("departed", "departed"),
      false => ("landed", "landed"),
    }
  } else if departing && now >= expected - 20 {
    ("boarding", "boarding")
  } else if row.delay > 0 && now >= row.at - 45 {
    ("delayed", "delayed")
  } else {
    ("on time", "on-time")
  };
  let gate = match row.moved {
    Some((at, gate)) if now >= at => gate,
    _ => row.gate,
  };
  let mut map = ValueMap::new();
  map.insert("flight".to_owned(), Value::str(row.flight));
  map.insert("city".to_owned(), Value::str(row.city));
  map.insert("scheduled".to_owned(), Value::str(&clock(row.at)));
  map.insert("expected".to_owned(), Value::str(&clock(expected)));
  map.insert("status".to_owned(), Value::str(status));
  map.insert("code".to_owned(), Value::str(code));
  map.insert("gate".to_owned(), Value::str(gate));
  Value::Map(map)
}

/// The board at `now`: everything that has not been gone an hour, in the
/// order the field shows it.
fn board(now: i64) -> Value {
  let of = |rows: &'static [Scheduled], departing: bool| {
    Value::Seq(
      rows
        .iter()
        .filter(|row| now < row.at + row.delay + 60)
        .map(|row| flight(row, now, departing))
        .collect(),
    )
  };
  let mut map = ValueMap::new();
  map.insert("at".to_owned(), Value::str(&clock(now)));
  map.insert("arrivals".to_owned(), of(ARRIVALS, false));
  map.insert("departures".to_owned(), of(DEPARTURES, true));
  Value::Map(map)
}

/// The field's own reading, drifting with the morning.
fn weather(now: i64) -> Value {
  let quarter = (now / 90).rem_euclid(FIELDS.len() as i64) as usize;
  let mut map = ValueMap::new();
  map.insert("field".to_owned(), Value::str(FIELDS[quarter]));
  map.insert("wind".to_owned(), Value::str(&format!("{}° at {} kt", (now / 7 % 36) * 10, 6 + now % 11)));
  map.insert("visibility".to_owned(), Value::str(&format!("{} km", 4 + (now / 13) % 7)));
  map.insert("celsius".to_owned(), Value::F64((8 + (now - 420) / 45) as f64));
  Value::Map(map)
}

/// Every gate change the field has announced by `now`, most recent first.
fn gates(now: i64) -> Value {
  let mut rows: Vec<(i64, Value)> = ARRIVALS
    .iter()
    .chain(DEPARTURES.iter())
    .filter_map(|row| row.moved.map(|(at, to)| (at, row, to)))
    .filter(|(at, _, _)| now >= *at)
    .map(|(at, row, to)| {
      let mut map = ValueMap::new();
      map.insert("flight".to_owned(), Value::str(row.flight));
      map.insert("was".to_owned(), Value::str(row.gate));
      map.insert("now".to_owned(), Value::str(to));
      map.insert("at".to_owned(), Value::str(&clock(at)));
      (at, Value::Map(map))
    })
    .collect();
  rows.sort_by_key(|(at, _)| -at);
  Value::Seq(rows.into_iter().map(|(_, row)| row).collect())
}

/// The field's three systems, in process. The board answers at once, the
/// weather takes `pause` and the gate system, which is the oldest thing on
/// the field, takes twice that.
fn transport(clock: Clock, pause: Duration) -> Arc<dyn Transport> {
  let (for_board, for_weather, for_gates) = (clock.clone(), clock.clone(), clock);
  Arc::new(
    LocalTransport::new()
      .method("board.getBoard", move |_| {
        let value = board(field_time(for_board.minutes()));
        async move { Ok(value) }
      })
      .method("board.getWeather", move |_| {
        let value = weather(field_time(for_weather.minutes()));
        async move {
          tokio::time::sleep(pause).await;
          Ok(value)
        }
      })
      .method("board.listGateChanges", move |_| {
        let value = gates(field_time(for_gates.minutes()));
        async move {
          tokio::time::sleep(pause * 2).await;
          Ok(value)
        }
      }),
  )
}

/// The field with its clock running: the morning starts at 07:10 and `speed`
/// simulated minutes pass every real second, wrapping every 200 minutes so
/// the morning repeats for as long as anyone is watching.
pub fn running(pause: Duration, speed: f64) -> Arc<dyn Transport> {
  transport(Clock::Running { from: Instant::now(), start: 430, speed }, pause)
}

/// The field stopped at one minute past midnight, which is what a test holds.
pub fn at(minutes: i64, pause: Duration) -> Arc<dyn Transport> {
  transport(Clock::Frozen(minutes), pause)
}
