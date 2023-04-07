use std::{fmt::Display, str::FromStr};

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellPosition {
    pub column: u32,
    pub row: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum CellPositionParseError {
    #[error("Invalid character in position string")]
    InvalidCharacter,
    #[error("Invalid format for position string")]
    InvalidFormat,
}

impl FromStr for CellPosition {
    type Err = CellPositionParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut column_str = String::new();
        let mut row_str = String::new();
        let mut found_numeric = false;

        for c in s.chars() {
            if c.is_alphabetic() {
                if found_numeric {
                    return Err(CellPositionParseError::InvalidFormat);
                }
                column_str.push(c);
            } else if c.is_numeric() {
                found_numeric = true;
                row_str.push(c);
            } else {
                return Err(CellPositionParseError::InvalidCharacter);
            }
        }

        if column_str.is_empty() || row_str.is_empty() {
            return Err(CellPositionParseError::InvalidFormat);
        }

        let column = column_str
            .chars()
            .rev()
            .enumerate()
            .map(|(i, c)| (c.to_ascii_uppercase() as u32 - 'A' as u32 + 1) * 26u32.pow(i as u32))
            .sum::<u32>()
            - 1;

        let row = row_str
            .parse::<u32>()
            .map_err(|_| CellPositionParseError::InvalidFormat)?
            - 1;

        Ok(CellPosition { column, row })
    }
}

impl<'de> Deserialize<'de> for CellPosition {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse::<CellPosition>().map_err(de::Error::custom)
    }
}

impl Into<(u32, u32)> for CellPosition {
    fn into(self) -> (u32, u32) {
        (self.row, self.column)
    }
}

impl From<(u32, u32)> for CellPosition {
    fn from(value: (u32, u32)) -> Self {
        Self {
            column: value.1,
            row: value.0,
        }
    }
}

fn number_to_excel_column(column: u32) -> String {
    let mut result = String::new();
    let mut col = column;

    while col >= 26 {
        let remainder = col % 26;
        col = col / 26 - 1;
        result.push((remainder as u8 + b'A') as char);
    }

    result.push((col as u8 + b'A') as char);
    result.chars().rev().collect::<String>()
}

impl Display for CellPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let col_label = number_to_excel_column(self.column);
        write!(f, "{}{}", col_label, self.row + 1)
    }
}

impl Serialize for CellPosition {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = self.to_string();
        serializer.serialize_str(&s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetCellPosition {
    pub sheet: String,
    pub position: CellPosition,
}

#[derive(Debug, thiserror::Error)]
pub enum SheetCellPositionParseError {
    #[error("Invalid format for sheet cell position string")]
    InvalidFormat,
    #[error(transparent)]
    CellPositionParseError(#[from] CellPositionParseError),
}

impl FromStr for SheetCellPosition {
    type Err = SheetCellPositionParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split('!');
        let sheet = parts
            .next()
            .ok_or(SheetCellPositionParseError::InvalidFormat)?
            .to_string();
        let position_str = parts
            .next()
            .ok_or(SheetCellPositionParseError::InvalidFormat)?;

        if parts.next().is_some() {
            return Err(SheetCellPositionParseError::InvalidFormat);
        }

        let position = position_str.parse::<CellPosition>()?;

        Ok(SheetCellPosition { sheet, position })
    }
}

impl<'de> Deserialize<'de> for SheetCellPosition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse::<SheetCellPosition>().map_err(de::Error::custom)
    }
}

impl Display for SheetCellPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}!{}", self.sheet, self.position)
    }
}

impl Serialize for SheetCellPosition {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = self.to_string();
        serializer.serialize_str(&s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_str_valid() {
        let cases = vec![
            ("A1", 0, 0),
            ("B2", 1, 1),
            ("Z1", 25, 0),
            ("AA1", 26, 0),
            ("AB20", 27, 19),
            ("aB2", 27, 1),
            ("Bb40", 53, 39),
            ("ZZ100", 701, 99),
        ];

        for (input, expected_col, expected_row) in cases {
            let position = input.parse::<CellPosition>().unwrap();
            assert_eq!(position.column, expected_col);
            assert_eq!(position.row, expected_row);
        }
    }

    #[test]
    fn test_from_str_invalid() {
        let cases = vec!["1A", "A", "1", "a", "AB", "A1B", "", " ", "A 1", "A1%"];

        for input in cases {
            dbg!(input);
            let result = input.parse::<CellPosition>();
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_sheet_cell_position_from_str_valid() {
        let cases = vec![
            ("Sheet1!A1", "Sheet1", 0, 0),
            ("Sheet2!B2", "Sheet2", 1, 1),
            ("Data!AB20", "Data", 27, 19),
        ];

        for (input, expected_sheet, expected_col, expected_row) in cases {
            let sheet_cell_position = input.parse::<SheetCellPosition>().unwrap();
            assert_eq!(sheet_cell_position.sheet, expected_sheet);
            assert_eq!(sheet_cell_position.position.column, expected_col);
            assert_eq!(sheet_cell_position.position.row, expected_row);
        }
    }

    #[test]
    fn test_sheet_cell_position_from_str_invalid() {
        let cases = vec![
            "Sheet1!1A",
            "Sheet1A1",
            "Sheet1!A",
            "Sheet1!1",
            "Sheet1!!A1",
            "Sheet1!",
            "",
        ];

        for input in cases {
            let result = input.parse::<SheetCellPosition>();
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_cell_position_round_trip() {
        let cases = vec![
            CellPosition { column: 0, row: 0 },
            CellPosition { column: 1, row: 1 },
            CellPosition {
                column: 26,
                row: 19,
            },
            CellPosition {
                column: 52,
                row: 25,
            },
        ];

        for position in cases {
            let serialized = serde_json::to_string(&position).unwrap();
            let deserialized: CellPosition = serde_json::from_str(&serialized).unwrap();

            assert_eq!(position, deserialized);
        }
    }

    #[test]
    fn test_sheet_cell_position_round_trip() {
        let cases = vec![
            SheetCellPosition {
                sheet: "Sheet1".to_string(),
                position: CellPosition { column: 0, row: 0 },
            },
            SheetCellPosition {
                sheet: "Sheet2".to_string(),
                position: CellPosition { column: 1, row: 1 },
            },
            SheetCellPosition {
                sheet: "Data".to_string(),
                position: CellPosition {
                    column: 26,
                    row: 19,
                },
            },
            SheetCellPosition {
                sheet: "Data".to_string(),
                position: CellPosition {
                    column: 52,
                    row: 25,
                },
            },
        ];

        for position in cases {
            let serialized = serde_json::to_string(&position).unwrap();
            let deserialized: SheetCellPosition = serde_json::from_str(&serialized).unwrap();

            assert_eq!(position, deserialized);
        }
    }
}
