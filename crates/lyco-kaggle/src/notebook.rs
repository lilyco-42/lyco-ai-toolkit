//! Build an nbformat notebook that the Kaggle CLI can push.
//!
//! Kaggle executes a pushed notebook on its own machines, so the notebook is
//! the transport. It is generated rather than hand-maintained because the
//! interesting parts of a generation kernel - which model, how many seeds,
//! which UV rectangles - should come from this repository, not from a cell
//! somebody edited in a browser.

use anyhow::Result;
use serde_json::{json, Value};

/// The Python version in the Kaggle GPU image.
const PYTHON_VERSION: &str = "3.12";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cell {
    Markdown(String),
    Code(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Notebook {
    cells: Vec<Cell>,
}

impl Notebook {
    pub fn new() -> Self {
        Notebook { cells: Vec::new() }
    }

    pub fn markdown(mut self, text: impl Into<String>) -> Self {
        self.cells.push(Cell::Markdown(text.into()));
        self
    }

    pub fn code(mut self, text: impl Into<String>) -> Self {
        self.cells.push(Cell::Code(text.into()));
        self
    }

    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// nbformat stores `source` as a list of lines, each keeping its newline.
    fn source_lines(text: &str) -> Vec<String> {
        let lines: Vec<String> = text.split_inclusive('\n').map(str::to_string).collect();
        if lines.is_empty() {
            return vec![String::new()];
        }
        lines
    }

    pub fn to_value(&self) -> Value {
        let cells: Vec<Value> = self
            .cells
            .iter()
            .map(|cell| match cell {
                Cell::Markdown(text) => json!({
                    "cell_type": "markdown",
                    "metadata": {},
                    "source": Self::source_lines(text),
                }),
                Cell::Code(text) => json!({
                    "cell_type": "code",
                    "metadata": {},
                    "execution_count": Value::Null,
                    "outputs": [],
                    "source": Self::source_lines(text),
                }),
            })
            .collect();

        json!({
            "nbformat": 4,
            "nbformat_minor": 5,
            "metadata": {
                "kernelspec": {
                    "display_name": "Python 3",
                    "language": "python",
                    "name": "python3",
                },
                "language_info": {
                    "name": "python",
                    "version": PYTHON_VERSION,
                },
            },
            "cells": cells,
        })
    }

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(&self.to_value())?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_nbformat_4_5() {
        let nb = Notebook::new().code("print(1)");
        let v = nb.to_value();
        assert_eq!(v["nbformat"], 4);
        assert_eq!(v["nbformat_minor"], 5);
        assert_eq!(v["metadata"]["kernelspec"]["name"], "python3");
        assert_eq!(v["metadata"]["language_info"]["name"], "python");
        assert!(v["cells"].is_array());
    }

    #[test]
    fn code_cells_need_outputs_and_an_execution_count() {
        let nb = Notebook::new().code("x = 1");
        let cell = &nb.to_value()["cells"][0];
        assert_eq!(cell["cell_type"], "code");
        assert_eq!(cell["execution_count"], Value::Null);
        assert_eq!(cell["outputs"], json!([]));
    }

    #[test]
    fn markdown_cells_carry_only_source() {
        let nb = Notebook::new().markdown("# hello");
        let cell = &nb.to_value()["cells"][0];
        assert_eq!(cell["cell_type"], "markdown");
        assert!(cell.get("outputs").is_none());
        assert!(cell.get("execution_count").is_none());
    }

    #[test]
    fn source_is_split_into_newline_terminated_lines() {
        let nb = Notebook::new().code("a\nb\nc");
        assert_eq!(
            nb.to_value()["cells"][0]["source"],
            json!(["a\n", "b\n", "c"])
        );
    }

    #[test]
    fn an_empty_code_cell_still_has_one_source_line() {
        let nb = Notebook::new().code("");
        assert_eq!(nb.to_value()["cells"][0]["source"], json!([""]));
    }

    #[test]
    fn round_trips_through_json() {
        let nb = Notebook::new()
            .markdown("title")
            .code("import torch\nprint(torch.cuda.is_available())\n");
        let text = nb.to_json().expect("serialize");
        let parsed: Value = serde_json::from_str(&text).expect("parse");
        assert_eq!(parsed["nbformat"], 4);
        assert_eq!(parsed["cells"].as_array().map(Vec::len), Some(2));
        // Reassembling the lines must give back the original source.
        let joined: String = parsed["cells"][1]["source"]
            .as_array()
            .expect("source array")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(joined, "import torch\nprint(torch.cuda.is_available())\n");
    }
}
