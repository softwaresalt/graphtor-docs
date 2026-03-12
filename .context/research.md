# **Local GraphRAG Plugin Blueprint: Dynamic Documentation (Rust-Native)**

This document outlines the end-to-end architecture and implementation plan for building a lightweight, highly performant, local Model Context Protocol (MCP) plugin written **100% in Rust**.

This system serves multi-source developer documentation (such as Microsoft, AWS, and proprietary local organization docs) directly to an AI agent using an embedded GraphRAG (Retrieval-Augmented Generation) architecture. By writing this entirely in Rust, we compile the entire ingestion engine, the local embedding model, the vector and graph databases, and the MCP server into a **single, zero-dependency executable binary**.

This approach completely eliminates external API latency, drastically reduces the context window bloat that plagues modern LLM agents, and guarantees absolute data privacy. Furthermore, it permanently solves the notorious "Python virtual environment" problem. A developer working in a C#, Go, Java, or Node.js workspace can seamlessly install and run this plugin without needing to configure Python interpreters, wrestle with pip dependencies, or install heavy machine learning libraries like PyTorch.

## **Phase 1: The "Zero-Dependency" Architecture Stack**

To ensure any user can dynamically index massive documentation repositories on standard laptop hardware without environment friction or excessive battery drain, the stack relies entirely on highly optimized, memory-safe Rust crates.

* **Vector Engine:** **LanceDB** (lancedb crate). This engine stores the raw markdown chunks and their mathematical vector embeddings. LanceDB's core is inherently written in Rust. By using the native Rust API, data flows from the embedding generation directly into the zero-copy Lance columnar format with maximum memory efficiency. Unlike traditional vector stores that must load all embeddings into RAM, LanceDB's disk-based indexing ensures your IDE remains incredibly responsive even when searching across gigabytes of documentation.  
* **Graph Engine:** **Kùzu** (kuzu crate). Kùzu acts as the structural brain of the operation, storing relationships such as hyperlinks, file hierarchies, and code block associations. While Kùzu's core is written in highly optimized C++, it provides first-class Rust bindings. It compiles directly into your binary, acting as an embedded graph database (analogous to SQLite for graphs) that executes blazing-fast Cypher queries. This allows the agent to perform complex multi-hop logic that vector math alone cannot solve.  
* **Native Embedding Model:** **Hugging Face Candle** (candle-core, candle-transformers). Instead of relying on heavy frameworks like PyTorch or standing up a separate Python server, we run the all-MiniLM-L6-v2 (~80MB) model natively using Candle, Hugging Face's minimalist, pure-Rust ML framework. It is extremely fast on standard CPUs, requires no external CUDA dependencies, and ensures the total binary footprint remains remarkably small.  
* **Graph Extraction:** **Deterministic Parsing** (pulldown-cmark crate). This is an industry-standard, highly compliant, pull-based Markdown parser. Rather than relying on a slow, error-prone LLM to extract relationships, pulldown-cmark generates an event stream (Abstract Syntax Tree) from the Markdown text in milliseconds. This allows us to deterministically extract nodes and edges directly from explicit Markdown hyperlinks and header hierarchies with 100% precision.  
* **Plugin Server:** **MCP SDK** (mcp-sdk or rmcp crates). A robust Rust implementation of the Model Context Protocol that handles the asynchronous JSON-RPC STDIO communication. It translates the AI agent's (e.g., Copilot, Cursor, or Claude Desktop) requests into native database queries seamlessly.

## **Phase 2: Dynamic Source Management & Incremental Sync**

Instead of hardcoding specific repositories and forcing all developers to download the same documentation, the system uses a user-managed registry and meticulously tracks synchronization state to adapt to each developer's specific needs.

### **Step A: The sources.yaml Registry**

Users define exactly what context they want their agent to have access to via a simple YAML configuration file (parsed safely in Rust using serde_yaml). This acts as the developer's personal knowledge manifesto, allowing them to mix open-source Git repositories with highly classified internal directories.

```text
sources:  
  # External public documentation  
  - id: ms-azure-core  
    type: git  
    url: [https://github.com/MicrosoftDocs/azure-docs](https://github.com/MicrosoftDocs/azure-docs)  
    branch: main  
    # Target specific directories to prevent context contamination from irrelevant products  
    include: ["articles/active-directory/**/*.md", "articles/azure-functions/**/*.md"]  
    # Skip noisy, non-production files  
    exclude: ["**/drafts/**", "**/deprecated/**"]

  # Internal, proprietary documentation  
  - id: internal-api-docs  
    type: local  
    path: /Users/dev/company-repo/docs  
    include: ["**/*.md"]
```

### **Step B: State Tracking for Updates**

Re-indexing massive documentation sets from scratch every time a file changes is computationally wasteful. The ingestion engine maintains a minimal .sync_state.json file (managed via serde_json) to enable surgical, incremental updates.

* For git sources, it stores the last processed Git commit hash.  
* For local sources, it tracks file modification timestamps (mtime).  
* When the user runs the compiled binary via graphtor-docs sync, the engine performs an in-memory differential analysis. It only drops and re-ingests the specific .md files that have been modified, added, or deleted. This reduces update times from tens of minutes to mere seconds, preventing "index churn" and preserving system resources.

## **Phase 3: The Native Ingestion Pipeline**

Because we removed the LLM from the ingestion phase, the pipeline relies entirely on the predictable, inherent structure of developer documentation to build the GraphRAG databases accurately and instantly.

### **Step A: Structural Parsing & Chunking**

Parse the markdown files using pulldown-cmark.

1. **Semantic Splitting:** Iterate over the AST event stream. When a Header event (like an H2 or H3) is encountered, create a new logical chunk. This ensures boundaries are drawn around complete thoughts (e.g., "Authentication Setup") rather than arbitrary character counts, resulting in vastly superior context for the LLM.  
2. **Edge Extraction:** When a Link event is encountered within a chunk, resolve the relative path. This link is stored as an explicit graph relationship (REFERENCES). This essentially pre-computes the map of the documentation for the LLM.  
3. **Asset Isolation:** When a CodeBlock event is encountered, extract it into an isolated node. This allows the agent to search specifically for code implementations without the noise of the surrounding conversational prose.

### **Step B: In-Process Vectorization (Candle)**

Pass the parsed text chunks directly to the Candle model. Because this happens in the same memory space as the parser, there is zero network overhead.

```text
// Conceptual Rust implementation using candle-transformers  
// The weights are loaded directly into memory without heavy ML runtimes  
let model = BertModel::load(weights, config)?;  
let tokens = tokenizer.encode("How to authenticate with Azure AD", true)?;

// Generates a dense 384-dimensional vector natively in Rust  
// This captures the semantic "meaning" of the chunk, not just keywords  
let embeddings = model.forward(&tokens)?; 
```

### **Step C: Graph Construction (Database Loading)**

The extracted structure is carefully bifurcated: topological data goes to Kùzu, while dense vectors and raw text go to LanceDB.

* **Nodes:** Document (the file), Chunk (the semantic text section), CodeSnippet (the isolated code).  
* **Edges:**  
  * `(Document)-[:CONTAINS]->(Chunk)`  
  * `(Chunk)-[:HAS_CODE]->(CodeSnippet)`  
  * `(Chunk)-[:REFERENCES]->(Document)` *(Derived natively from pulldown-cmark link events, enabling hyperlinked traversal!)*

## **Phase 4: Database Schema Design**

The dual-database architecture requires precise schema definitions to ensure lightning-fast joins across the vector and graph spaces.

### **Kùzu Property Graph Schema (Cypher)**

The Cypher schema maps the conceptual architecture of the documentation, allowing the LLM to understand how pieces fit together.

```sql
// Define Node Tables  
CREATE NODE TABLE SourceRepo (id STRING, url STRING, PRIMARY KEY (id));  
CREATE NODE TABLE Document (path STRING, title STRING, repo_id STRING, PRIMARY KEY (path));  
CREATE NODE TABLE Chunk (chunk_id STRING, heading STRING, PRIMARY KEY (chunk_id));  
CREATE NODE TABLE CodeSnippet (snippet_id STRING, language STRING, code STRING, PRIMARY KEY (snippet_id));

// Define Edge Tables  
CREATE REL TABLE BELONGS_TO (FROM Document TO SourceRepo);  
CREATE REL TABLE CONTAINS_CHUNK (FROM Document TO Chunk);  
CREATE REL TABLE HAS_CODE (FROM Chunk TO CodeSnippet);

// Crucial: Enables hyper-linked traversal for multi-hop reasoning  
CREATE REL TABLE REFERENCES (FROM Chunk TO Document); 
```

### **LanceDB Schema (Apache Arrow in Rust)**

Using the arrow crate ensures that memory is perfectly aligned with LanceDB's expectations, avoiding costly data copying.

```rust
use arrow::datatypes::{DataType, Field, Schema};  
use std::sync::Arc;

let schema = Arc::new(Schema::new(vec![  
    // The foreign key linking the vector directly back to the Kùzu Chunk node  
    Field::new("chunk_id", DataType::Utf8, false),   
      
    // Allows hard-filtering by specific docs (e.g., searching only 'internal-api-docs')  
    // This provides critical tenant isolation within the database  
    Field::new("repo_id", DataType::Utf8, false),  
      
    // The 384 dimensions matching the MiniLM output  
    Field::new("vector", DataType::FixedSizeList(  
        Arc::new(Field::new("item", DataType::Float32, true)),   
        384   
    ), false),  
      
    // The actual markdown content injected into the Copilot prompt  
    Field::new("text", DataType::Utf8, false),  
    Field::new("path", DataType::Utf8, false),  
]));
```

## **Phase 5: MCP Plugin Implementation**

Using an async Rust runtime like tokio and an MCP SDK crate, we expose the dual databases to the AI agent. The true power of this system lies in how the LLM orchestrates these two tools to form complete answers.

### **Tool 1:** `search_local_docs`

* **Description:** "Search your ingested documentation by semantic meaning. Use this as your primary entry point to find explanations, tutorials, or code references. You can optionally filter by repo_id (e.g., 'internal-api-docs' or 'ms-azure-core') to narrow scope."  
* **Action:** Embeds the user's query via the local Candle model, performs an Approximate Nearest Neighbor (ANN) search in LanceDB natively, and returns the top relevant chunks. This gives the agent its initial foothold on the topic.

### **Tool 2:** `traverse_doc_links`

* **Description:** "If a previously retrieved documentation chunk references another topic, use this tool to follow the documentation links and gather prerequisite, architectural, or related information."  
* **Action:** Executes a Kùzu Cypher query via the Rust bindings to find all documents linked *from* that chunk.  
  * **Implication:** If Tool 1 returns a chunk about deploying a web app, but that chunk links to a "Database Configuration" page, the LLM is smart enough to use Tool 2 to "click" that link, pulling the database configuration into context *before* answering the user.

```rust
// Traverses the graph from the target chunk, across the REFERENCES edge,   
// to the related document, and pulls down all associated chunks.  
MATCH (c:Chunk {chunk_id: $target_id})-[:REFERENCES]->(d:Document)-[:CONTAINS_CHUNK]->(linked_chunks:Chunk)  
RETURN linked_chunks.chunk_id, linked_chunks.heading
```

## **Phase 6: Example: Incremental Sync Logic in Rust**

Here is how the differential sync logic looks using the git2 crate. By using native Git bindings instead of shelling out to the system's git CLI, the application remains perfectly portable and highly performant, calculating diffs purely in memory.

```rust
use git2::Repository;  
use serde::{Deserialize, Serialize};  
use std::fs;  
use std::path::Path;

// Represents the on-disk state of our ingestion pipeline  
#[derive(Serialize, Deserialize, Default)]  
struct SyncState {  
    last_commit: Option<String>,  
}

fn sync_repository(repo_id: &str, local_path: &Path) -> Result<(), git2::Error> {  
    let repo = Repository::open(local_path)?;  
    let head = repo.head()?.peel_to_commit()?;  
    let latest_commit_id = head.id().to_string();

    // Load existing state to determine what has changed since the last run  
    let state_file = Path::new(".sync_state.json");  
    let mut state: SyncState = if state_file.exists() {  
        let data = fs::read_to_string(state_file).unwrap();  
        serde_json::from_str(&data).unwrap_or_default()  
    } else {  
        SyncState::default()  
    };

    // Fast exit if the repository hasn't changed, saving CPU cycles  
    if state.last_commit.as_ref() == Some(&latest_commit_id) {  
        println!("[{}] Up to date. No indexing required.", repo_id);  
        return Ok(());  
    }

    println!("[{}] Changes detected. Analyzing git diff...", repo_id);

    if let Some(last_commit_str) = &state.last_commit {  
        let old_tree = repo.find_commit(git2::Oid::from_str(last_commit_str)?)?.tree()?;  
        let new_tree = head.tree()?;  
          
        // Calculate the exact difference between the two trees  
        let diff = repo.diff_tree_to_tree(Some(&old_tree), Some(&new_tree), None)?;

        diff.print(git2::DiffFormat::NameOnly, |_delta, _hunk, line| {  
            let file_path = String::from_utf8_lossy(line.content()).trim().to_string();  
            if file_path.ends_with(".md") {  
                println!(" -> Re-ingesting modified file: {}", file_path);  
                  
                // Surgical update logic:  
                // 1. Query LanceDB to delete rows where `path` == file_path  
                // 2. Query Kuzu to detach/delete Document nodes where `path` == file_path  
                // 3. Parse the new file via pulldown-cmark, vectorize via Candle, insert to DBs  
            }  
            true  
        })?;  
    } else {  
        println!("[{}] First run detected. Ingesting full repository...", repo_id);  
        // Run full parsing loop over the directory tree for initial database population  
    }

    // Persist new state to disk  
    state.last_commit = Some(latest_commit_id);  
    let serialized = serde_json::to_string_pretty(&state).unwrap();  
    fs::write(state_file, serialized).unwrap();

    println!("[{}] Sync successfully completed.", repo_id);  
    Ok(())  
}  
```
