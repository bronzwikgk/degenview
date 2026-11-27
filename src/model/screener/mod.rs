//! Pure query language and daily-bar metrics for the Local NSE screener.

use crate::model::Kline;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Query {
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Comparison {
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    Equal,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Field {
    Open,
    High,
    Low,
    Close,
    Volume,
    Return(usize),
    Sma(usize),
    Ema(usize),
    Rsi(usize),
    /// Calendar month turnover, where 0 is the month containing the latest daily bar.
    MonthTurnover(usize),
    /// Mean calendar-week turnover across the most recent N weeks, including the latest bar's week.
    AvgWeekTurnover(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Operand {
    Number(f64),
    Field(Field),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Clause {
    pub left: Field,
    pub comparison: Comparison,
    pub right: Operand,
}

/// `any_of` is OR; every inner group is AND. AND binds tighter than OR.
#[derive(Clone, Debug, PartialEq)]
pub struct Expression {
    pub any_of: Vec<Vec<Clause>>,
}

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum QueryError {
    #[error("query is empty")]
    Empty,
    #[error("invalid condition `{0}`")]
    Invalid(String),
    #[error("unsupported field `{0}`")]
    Field(String),
    #[error("period must be a positive whole number")]
    Period,
    #[error("right side must be a field or number")]
    Value,
}

/// Parses case-insensitive `AND` / `OR` conditions, e.g.
/// `close > ema(20) and rsi(14) > 50 OR return(5) >= 3`.
pub fn parse(query: &str) -> Result<Expression, QueryError> {
    let text = query.trim();
    if text.is_empty() {
        return Err(QueryError::Empty);
    }
    let any_of = split_keyword(text, "OR")
        .into_iter()
        .map(|group| {
            split_keyword(group, "AND")
                .into_iter()
                .map(parse_clause)
                .collect()
        })
        .collect::<Result<Vec<Vec<Clause>>, QueryError>>()?;
    if any_of.iter().any(Vec::is_empty) {
        return Err(QueryError::Invalid(text.to_owned()));
    }
    Ok(Expression { any_of })
}

fn split_keyword<'a>(text: &'a str, keyword: &str) -> Vec<&'a str> {
    let needle = format!(" {keyword} ");
    let upper = text.to_ascii_uppercase();
    let mut pieces = Vec::new();
    let mut start = 0;
    while let Some(relative) = upper[start..].find(&needle) {
        let end = start + relative;
        pieces.push(text[start..end].trim());
        start = end + needle.len();
    }
    pieces.push(text[start..].trim());
    pieces
}

fn parse_clause(text: &str) -> Result<Clause, QueryError> {
    let operators = [
        (">=", Comparison::GreaterEqual),
        ("<=", Comparison::LessEqual),
        ("=", Comparison::Equal),
        (">", Comparison::Greater),
        ("<", Comparison::Less),
    ];
    let (left, comparison, right) = operators
        .into_iter()
        .find_map(|(operator, comparison)| {
            text.split_once(operator)
                .map(|(left, right)| (left.trim(), comparison, right.trim()))
        })
        .ok_or_else(|| QueryError::Invalid(text.to_owned()))?;
    if left.is_empty() || right.is_empty() {
        return Err(QueryError::Invalid(text.to_owned()));
    }
    let right = match right.parse::<f64>() {
        Ok(value) if value.is_finite() => Operand::Number(value),
        _ => Operand::Field(parse_field(right)?),
    };
    Ok(Clause {
        left: parse_field(left)?,
        comparison,
        right,
    })
}

fn parse_field(text: &str) -> Result<Field, QueryError> {
    let lower = text.trim().to_ascii_lowercase();
    match lower.as_str() {
        "open" => Ok(Field::Open),
        "high" => Ok(Field::High),
        "low" => Ok(Field::Low),
        "close" => Ok(Field::Close),
        "volume" => Ok(Field::Volume),
        _ => {
            for (name, make) in [
                ("return", Field::Return as fn(usize) -> Field),
                ("sma", Field::Sma),
                ("ema", Field::Ema),
                ("rsi", Field::Rsi),
                ("month_turnover", Field::MonthTurnover),
                ("avg_week_turnover", Field::AvgWeekTurnover),
            ] {
                if let Some(period) = lower
                    .strip_prefix(&format!("{name}("))
                    .and_then(|rest| rest.strip_suffix(')'))
                {
                    let period = period.trim().parse().map_err(|_| QueryError::Period)?;
                    return (period > 0)
                        .then_some(make(period))
                        .ok_or(QueryError::Period);
                }
            }
            Err(QueryError::Field(text.to_owned()))
        }
    }
}

pub fn matches(expression: &Expression, bars: &[Kline]) -> bool {
    expression.any_of.iter().any(|group| {
        group.iter().all(|clause| {
            let Some(left) = value(&clause.left, bars) else {
                return false;
            };
            let right = match &clause.right {
                Operand::Number(value) => Some(*value),
                Operand::Field(field) => value(field, bars),
            };
            right.is_some_and(|right| match clause.comparison {
                Comparison::Greater => left > right,
                Comparison::GreaterEqual => left >= right,
                Comparison::Less => left < right,
                Comparison::LessEqual => left <= right,
                Comparison::Equal => (left - right).abs() < 1e-9,
            })
        })
    })
}

/// Minimum history needed to evaluate all fields, so scans do not always read 260 bars.
pub fn required_bars(expression: &Expression) -> usize {
    expression
        .any_of
        .iter()
        .flatten()
        .flat_map(|clause| {
            std::iter::once(&clause.left).chain(match &clause.right {
                Operand::Field(field) => Some(field),
                Operand::Number(_) => None,
            })
        })
        .map(field_bars)
        .max()
        .unwrap_or(1)
}
fn field_bars(field: &Field) -> usize {
    match field {
        Field::Return(n) | Field::Rsi(n) => n + 1,
        Field::Sma(n) | Field::Ema(n) => *n,
        Field::MonthTurnover(n) => (n + 1) * 31 + 10,
        Field::AvgWeekTurnover(n) => n * 7 + 10,
        _ => 1,
    }
}

pub fn value(field: &Field, bars: &[Kline]) -> Option<f64> {
    let last = bars.last()?;
    match *field {
        Field::Open => Some(last.open),
        Field::High => Some(last.high),
        Field::Low => Some(last.low),
        Field::Close => Some(last.close),
        Field::Volume => Some(last.volume),
        Field::Return(period) => bars
            .get(bars.len().checked_sub(period + 1)?)
            .filter(|bar| bar.close != 0.0)
            .map(|bar| (last.close / bar.close - 1.0) * 100.0),
        Field::Sma(period) => average(bars, period),
        Field::Ema(period) => ema(bars, period),
        Field::Rsi(period) => rsi_wilder(bars, period),
        Field::MonthTurnover(age) => month_turnover(bars, age),
        Field::AvgWeekTurnover(weeks) => avg_week_turnover(bars, weeks),
    }
}
fn average(bars: &[Kline], period: usize) -> Option<f64> {
    let slice = bars.get(bars.len().checked_sub(period)?..)?;
    Some(slice.iter().map(|bar| bar.close).sum::<f64>() / period as f64)
}
fn ema(bars: &[Kline], period: usize) -> Option<f64> {
    let first = bars.get(bars.len().checked_sub(period)?)?;
    let alpha = 2.0 / (period as f64 + 1.0);
    Some(
        bars[bars.len() - period + 1..]
            .iter()
            .fold(first.close, |current, bar| {
                alpha * bar.close + (1.0 - alpha) * current
            }),
    )
}
fn rsi_wilder(bars: &[Kline], period: usize) -> Option<f64> {
    let pairs: Vec<_> = bars.windows(2).collect();
    if pairs.len() < period {
        return None;
    }
    let (gain, loss) =
        pairs[pairs.len() - period..]
            .iter()
            .fold((0.0, 0.0), |(gain, loss), pair| {
                let change = pair[1].close - pair[0].close;
                (gain + change.max(0.0), loss + (-change).max(0.0))
            });
    if loss == 0.0 {
        Some(100.0)
    } else {
        let rs = gain / loss;
        Some(100.0 - 100.0 / (1.0 + rs))
    }
}

fn turnover(bar: &Kline) -> f64 {
    bar.close * bar.volume
}

/// Gregorian calendar year/month from UTC epoch milliseconds. Daily local CSV bars are midnight
/// UTC, so this deliberately does not depend on the device timezone.
fn year_month(ms: i64) -> (i32, u32) {
    let z = ms.div_euclid(86_400_000) + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };
    (year as i32, month as u32)
}

fn month_turnover(bars: &[Kline], age: usize) -> Option<f64> {
    let (latest_year, latest_month) = year_month(bars.last()?.close_time);
    let target = latest_year as i64 * 12 + latest_month as i64 - 1 - age as i64;
    let total: f64 = bars
        .iter()
        .filter(|bar| {
            let (year, month) = year_month(bar.close_time);
            year as i64 * 12 + month as i64 - 1 == target
        })
        .map(turnover)
        .sum();
    (total > 0.0).then_some(total)
}

fn avg_week_turnover(bars: &[Kline], weeks: usize) -> Option<f64> {
    if weeks == 0 {
        return None;
    }
    let latest_day = bars.last()?.close_time.div_euclid(86_400_000);
    // 1970-01-01 was Thursday; this maps every day to the Monday of its ISO-style week.
    let latest_week = (latest_day - (latest_day + 3).rem_euclid(7)) / 7;
    let start = latest_week - weeks as i64 + 1;
    let mut totals = vec![0.0; weeks];
    for bar in bars {
        let day = bar.close_time.div_euclid(86_400_000);
        let week = (day - (day + 3).rem_euclid(7)) / 7;
        if (start..=latest_week).contains(&week) {
            totals[(week - start) as usize] += turnover(bar);
        }
    }
    totals
        .iter()
        .all(|value| *value > 0.0)
        .then(|| totals.iter().sum::<f64>() / weeks as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bar(close: f64) -> Kline {
        Kline {
            open_time: 0,
            close_time: 0,
            open: close,
            high: close,
            low: close,
            close,
            volume: 100.0,
        }
    }
    #[test]
    fn price_can_compare_to_ema() {
        let expression = parse("close > ema(3) AND rsi(2) >= 50").unwrap();
        assert_eq!(required_bars(&expression), 3);
        assert!(matches(&expression, &[bar(1.0), bar(2.0), bar(3.0)]));
    }
    #[test]
    fn operators_are_case_insensitive_and_or_grouped() {
        let expression = parse("return(1) > 20 or close = 2 AND volume > 1").unwrap();
        assert!(matches(&expression, &[bar(1.0), bar(2.0)]));
    }
    #[test]
    fn rejects_empty_period() {
        assert_eq!(parse("close > ema(0)").unwrap_err(), QueryError::Period);
    }
}
