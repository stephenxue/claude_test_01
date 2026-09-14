//! LanceDB-backed vector store for document chunks.
//!
//! API shape verified against the `lancedb` 0.38 / `arrow` 58 official
//! example (rust/lancedb/examples/simple.rs in the lancedb repo). Note the
//! two traits imported below (`ExecutableQuery`, `QueryBase`) - the
//! `.execute()` / `.nearest_to()` / `.limit()` builder methods live on
//! these traits, not directly on the query struct, so without importing
//! them `cargo build` fails with "no method named `nearest_to` found".

use anyhow::{anyhow, Context, Result};
use arrow_array::types::Float32Type;
use arrow_array::{Array, FixedSizeListArray, Int32Array, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use futures_util::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

const TABLE_NAME: &str = "documents";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkRecord {
    pub id: String,
    pub file_name: String,
    pub chunk_index: i32,
    pub chunk_text: String,
    pub vector: Vec<f32>,
}

pub struct VectorDb {
    connection: lancedb::Connection,
    dim: i32,
}

fn schema(dim: i32) -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("file_name", DataType::Utf8, false),
        Field::new("chunk_index", DataType::Int32, false),
        Field::new("chunk_text", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), dim),
            false,
        ),
    ]))
}

fn records_to_batch(records: &[ChunkRecord], dim: i32) -> Result<RecordBatch> {
    let ids = StringArray::from_iter_values(records.iter().map(|r| r.id.as_str()));
    let file_names = StringArray::from_iter_values(records.iter().map(|r| r.file_name.as_str()));
    let chunk_indices = Int32Array::from_iter_values(records.iter().map(|r| r.chunk_index));
    let chunk_texts = StringArray::from_iter_values(records.iter().map(|r| r.chunk_text.as_str()));

    let vectors = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
        records
            .iter()
            .map(|r| Some(r.vector.iter().map(|v| Some(*v)).collect::<Vec<_>>())),
        dim,
    );

    RecordBatch::try_new(
        schema(dim),
        vec![
            Arc::new(ids) as arrow_array::ArrayRef,
            Arc::new(file_names) as arrow_array::ArrayRef,
            Arc::new(chunk_indices) as arrow_array::ArrayRef,
            Arc::new(chunk_texts) as arrow_array::ArrayRef,
            Arc::new(vectors) as arrow_array::ArrayRef,
        ],
    )
    .context("构建 RecordBatch 失败")
}

impl VectorDb {
    pub async fn open(db_path: &Path, dim: i32) -> Result<Self> {
        std::fs::create_dir_all(db_path).context("创建向量数据库目录失败")?;
        let connection = lancedb::connect(db_path.to_string_lossy().as_ref())
            .execute()
            .await
            .context("连接 LanceDB 失败")?;
        Ok(Self { connection, dim })
    }

    async fn table_exists(&self) -> Result<bool> {
        let names = self.connection.table_names().execute().await?;
        Ok(names.iter().any(|n| n == TABLE_NAME))
    }

    /// Inserts a batch of chunks, creating the table on first use.
    pub async fn add_chunks(&self, records: &[ChunkRecord]) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        let batch = records_to_batch(records, self.dim)?;

        if self.table_exists().await? {
            let table = self.connection.open_table(TABLE_NAME).execute().await?;
            table.add(batch).execute().await?;
        } else {
            self.connection
                .create_table(TABLE_NAME, batch)
                .execute()
                .await?;
        }
        Ok(())
    }

    /// Removes every chunk previously indexed for this file name (used both
    /// when a file is deleted via the app's delete button and when a file is
    /// re-ingested/restored, to avoid duplicate stale chunks).
    pub async fn delete_by_file_name(&self, file_name: &str) -> Result<()> {
        if !self.table_exists().await? {
            return Ok(());
        }
        let table = self.connection.open_table(TABLE_NAME).execute().await?;
        let escaped = file_name.replace('\'', "''");
        table
            .delete(&format!("file_name = '{escaped}'"))
            .await
            .context("从向量数据库删除失败")?;
        Ok(())
    }

    pub async fn search(&self, query_vector: Vec<f32>, k: usize) -> Result<Vec<ChunkRecord>> {
        if !self.table_exists().await? {
            return Ok(vec![]);
        }
        let table = self.connection.open_table(TABLE_NAME).execute().await?;
        let stream = table
            .query()
            .nearest_to(query_vector.as_slice())?
            .limit(k)
            .execute()
            .await
            .context("向量检索失败")?;
        let batches: Vec<RecordBatch> = stream.try_collect().await.context("读取检索结果失败")?;

        let mut results = Vec::new();
        for batch in batches {
            let ids = batch
                .column_by_name("id")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| anyhow!("结果缺少 id 列"))?;
            let file_names = batch
                .column_by_name("file_name")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| anyhow!("结果缺少 file_name 列"))?;
            let chunk_indices = batch
                .column_by_name("chunk_index")
                .and_then(|c| c.as_any().downcast_ref::<Int32Array>())
                .ok_or_else(|| anyhow!("结果缺少 chunk_index 列"))?;
            let chunk_texts = batch
                .column_by_name("chunk_text")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| anyhow!("结果缺少 chunk_text 列"))?;

            for i in 0..batch.num_rows() {
                results.push(ChunkRecord {
                    id: ids.value(i).to_string(),
                    file_name: file_names.value(i).to_string(),
                    chunk_index: chunk_indices.value(i),
                    chunk_text: chunk_texts.value(i).to_string(),
                    vector: vec![], // not needed by callers; omitted to avoid re-parsing the FixedSizeList
                });
            }
        }
        Ok(results)
    }
}
