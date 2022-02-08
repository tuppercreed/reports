use crate::pre_process::block::{Command, Expression, ExpressionVariable};
use anyhow::{Error, Result};

/// Table is two dimensional structure that adds one [ExpressionVariable] from each dimension
/// into an [Expression] to complete it.  
///
/// A row or column adds the *same* [ExpressionVariable] to the entire row/column,
/// so that a cross-section of two variables is generated.
pub struct Table<'a> {
    rows: Vec<ExpressionVariable<'a>>,
    cols: Vec<ExpressionVariable<'a>>,
    ctx: &'a Expression<'a>,
}

impl<'a> Table<'a> {
    pub fn new(
        rows: Vec<ExpressionVariable<'a>>,
        cols: Vec<ExpressionVariable<'a>>,
        ctx: &'a Expression<'a>,
    ) -> Self {
        Self { rows, cols, ctx }
    }
}

impl TryFrom<Table<'_>> for String {
    type Error = Error;

    fn try_from(value: Table<'_>) -> Result<Self, Self::Error> {
        let mut table = String::from("\n| _ |");
        for col in &value.cols {
            // Column Headers
            table.push_str(&format!(" {} |", &col.to_string()));
        }
        table.push_str("\n|");
        for _ in 0..&value.cols.len() + 1 {
            table.push_str(" --- |");
        }
        for row in &value.rows {
            let ctx = value.ctx + row.clone();
            // Row Heading
            table.push_str(&format!("\n| {} |", &row.to_string()));
            for col in &value.cols {
                let mut ctx = &ctx + col.clone();
                table.push_str(&format!(
                    " {} |",
                    &String::try_from(Command::try_from(&mut ctx)?)?
                ));
            }
        }
        table.push_str("\n");
        Ok(table)
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;
    use crate::{Data, TimeFrequency};

    #[test]
    fn test_table() {
        let date = NaiveDate::from_ymd(2022, 2, 4);
        let mut ctx = Expression::new();
        ctx.set_data(Data::read(&"cat_purrs".to_string(), &date).unwrap());
        let cols = vec![
            ExpressionVariable::TimeFrequency(TimeFrequency::Weekly),
            ExpressionVariable::TimeFrequency(TimeFrequency::Quarterly),
        ];
        let rows = vec![
            ExpressionVariable::Command("change"),
            ExpressionVariable::Command("avg_freq"),
        ];
        let tbl = Table::new(rows, cols, &ctx);
        let tbl = String::try_from(tbl).unwrap();

        assert_eq!(
            tbl,
            "
| _ | Weekly | Quarterly |
| --- | --- | --- |
| change | 25.0% | 233.3% |
| avg_freq | 10.0 | 130.0 |
"
            .to_string()
        );
    }
}
