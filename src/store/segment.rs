//! Parquet segment files. Each property leaf path is split into three columns:
//! `{path}#t` (type tag), `{path}#n` (number) and `{path}#s` (string, or JSON text for lists),
//! next to `_ts`, `_id` and `_raw` (the full event JSON, kept for extraction and reprocessing).

use super::table::{leaves, Cell};
use anyhow::{anyhow, Result};
use arrow_array::{Array, ArrayRef, Float64Array, Int64Array, RecordBatch, StringArray, UInt8Array};
use arrow_schema::{DataType, Field, Schema};
use bytes::Bytes;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::{ArrowWriter, ProjectionMask};
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::WriterProperties;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

const T_MISSING: u8 = 0;
const T_NULL: u8 = 1;
const T_FALSE: u8 = 2;
const T_TRUE: u8 = 3;
const T_NUM: u8 = 4;
const T_STR: u8 = 5;
const T_JSON: u8 = 6;

/// One event ready to be written.
pub struct Row<'a> {
    pub ts: i64,
    pub id: &'a str,
    pub raw: &'a str,
    pub event: &'a Value,
}

/// Encodes rows (already sorted by timestamp) as a Parquet file.
pub fn write(rows: &[Row]) -> Result<Bytes> {
    let mut paths: BTreeMap<String, ()> = BTreeMap::new();
    let mut scratch = Vec::new();
    for r in rows {
        scratch.clear();
        leaves(r.event, "", &mut scratch);
        for (p, _) in scratch.drain(..) {
            paths.insert(p, ());
        }
    }
    let mut fields = vec![
        Field::new("_ts", DataType::Int64, false),
        Field::new("_id", DataType::Utf8, false),
        Field::new("_raw", DataType::Utf8, false),
    ];
    let mut cols: Vec<ArrayRef> = vec![
        Arc::new(Int64Array::from_iter_values(rows.iter().map(|r| r.ts))),
        Arc::new(StringArray::from_iter_values(rows.iter().map(|r| r.id))),
        Arc::new(StringArray::from_iter_values(rows.iter().map(|r| r.raw))),
    ];
    for p in paths.keys() {
        let mut tags = Vec::with_capacity(rows.len());
        let mut nums: Vec<Option<f64>> = Vec::with_capacity(rows.len());
        let mut strs: Vec<Option<String>> = Vec::with_capacity(rows.len());
        for r in rows {
            let (t, n, s) = match super::table::lookup(r.event, p) {
                None => (T_MISSING, None, None),
                Some(Value::Null) => (T_NULL, None, None),
                Some(Value::Bool(b)) => (if *b { T_TRUE } else { T_FALSE }, None, None),
                Some(Value::Number(n)) => (T_NUM, n.as_f64(), None),
                Some(Value::String(s)) => (T_STR, None, Some(s.clone())),
                Some(v) => (T_JSON, None, Some(v.to_string())),
            };
            tags.push(t);
            nums.push(n);
            strs.push(s);
        }
        fields.push(Field::new(format!("{p}#t"), DataType::UInt8, false));
        fields.push(Field::new(format!("{p}#n"), DataType::Float64, true));
        fields.push(Field::new(format!("{p}#s"), DataType::Utf8, true));
        cols.push(Arc::new(UInt8Array::from(tags)));
        cols.push(Arc::new(Float64Array::from(nums)));
        cols.push(Arc::new(StringArray::from(strs)));
    }
    let schema = Arc::new(Schema::new(fields));
    let batch = RecordBatch::try_new(schema.clone(), cols)?;
    let props = WriterProperties::builder()
        .set_compression(Compression::ZSTD(ZstdLevel::try_new(3)?))
        .set_max_row_group_row_count(Some(256 * 1024))
        .build();
    let mut buf = Vec::new();
    let mut w = ArrowWriter::try_new(&mut buf, schema, Some(props))?;
    w.write(&batch)?;
    w.close()?;
    Ok(Bytes::from(buf))
}

/// A decoded view of the parts of a segment a query needs.
pub struct Decoded {
    pub ts: Vec<i64>,
    pub ids: Vec<Arc<str>>,
    pub cols: HashMap<String, Vec<Cell>>,
    pub raw: Option<Vec<Arc<str>>>,
}

/// Leaf paths stored in a segment, from its schema.
pub fn stored_paths(data: &Bytes) -> Result<Vec<String>> {
    let b = ParquetRecordBatchReaderBuilder::try_new(data.clone())?;
    Ok(b.schema()
        .fields()
        .iter()
        .filter_map(|f| f.name().strip_suffix("#t").map(str::to_string))
        .collect())
}

/// Reads `_ts`, `_id`, the given leaf paths, and optionally `_raw`.
pub fn read(data: &Bytes, paths: &[String], want_raw: bool) -> Result<Decoded> {
    let builder = ParquetRecordBatchReaderBuilder::try_new(data.clone())?;
    let schema = builder.schema().clone();
    let mut wanted: Vec<String> = vec!["_ts".into(), "_id".into()];
    if want_raw {
        wanted.push("_raw".into());
    }
    for p in paths {
        for suf in ["#t", "#n", "#s"] {
            wanted.push(format!("{p}{suf}"));
        }
    }
    let indices: Vec<usize> = wanted.iter().filter_map(|n| schema.index_of(n).ok()).collect();
    let mask = ProjectionMask::roots(builder.parquet_schema(), indices);
    let reader = builder.with_projection(mask).with_batch_size(64 * 1024).build()?;
    let mut out = Decoded { ts: vec![], ids: vec![], cols: HashMap::new(), raw: if want_raw { Some(vec![]) } else { None } };
    for p in paths {
        out.cols.insert(p.clone(), Vec::new());
    }
    for batch in reader {
        let batch = batch?;
        let get = |name: &str| batch.column_by_name(name).cloned();
        let ts = get("_ts").ok_or_else(|| anyhow!("segment without _ts"))?;
        let ts = ts.as_any().downcast_ref::<Int64Array>().ok_or_else(|| anyhow!("bad _ts"))?;
        out.ts.extend(ts.values().iter().copied());
        let ids = get("_id").ok_or_else(|| anyhow!("segment without _id"))?;
        let ids = ids.as_any().downcast_ref::<StringArray>().ok_or_else(|| anyhow!("bad _id"))?;
        out.ids.extend(ids.iter().map(|s| Arc::from(s.unwrap_or(""))));
        if let Some(raw) = out.raw.as_mut() {
            let r = get("_raw").ok_or_else(|| anyhow!("segment without _raw"))?;
            let r = r.as_any().downcast_ref::<StringArray>().ok_or_else(|| anyhow!("bad _raw"))?;
            raw.extend(r.iter().map(|s| Arc::from(s.unwrap_or(""))));
        }
        for p in paths {
            let col = out.cols.get_mut(p).unwrap();
            let (Some(t), Some(n), Some(s)) = (get(&format!("{p}#t")), get(&format!("{p}#n")), get(&format!("{p}#s"))) else {
                col.extend(std::iter::repeat_n(Cell::Missing, batch.num_rows()));
                continue;
            };
            let t = t.as_any().downcast_ref::<UInt8Array>().ok_or_else(|| anyhow!("bad tag column"))?;
            let n = n.as_any().downcast_ref::<Float64Array>().ok_or_else(|| anyhow!("bad num column"))?;
            let s = s.as_any().downcast_ref::<StringArray>().ok_or_else(|| anyhow!("bad str column"))?;
            for i in 0..batch.num_rows() {
                col.push(match t.value(i) {
                    T_NULL => Cell::Null,
                    T_FALSE => Cell::Bool(false),
                    T_TRUE => Cell::Bool(true),
                    T_NUM => Cell::Num(n.value(i)),
                    T_STR => Cell::Str(Arc::from(s.value(i))),
                    T_JSON => Cell::Json(Arc::new(serde_json::from_str(s.value(i)).unwrap_or(Value::Null))),
                    _ => Cell::Missing,
                });
            }
        }
    }
    Ok(out)
}
