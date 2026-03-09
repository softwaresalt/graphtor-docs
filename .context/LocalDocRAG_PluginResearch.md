3/8/26, 9:33 PM

Google Gemini

Local GraphRAG Plugin Blueprint: Microsoft Docs

This document outlines the end-to-end architecture and implementation plan for building a

local Model Context Protocol (MCP) plugin that serves Microsoft Documentation to an AI agent

using an embedded GraphRAG architecture.

Phase 1: Architecture Stack

To maintain the absolute lightest footprint and highest performance, the stack utilizes highly

specialized embedded databases.

Vector Engine: LanceDB (Embedded, Python/Rust). Stores the raw markdown chunks and

their vector embeddings.

Graph Engine: Kùzu (Embedded, Python/Rust/C++). Stores the structural relationships

between APIs, Services, and SDKs.

Embedding Model: nomic-embed-text  (via local Ollama). Highly efficient context window

(8k) for code and documentation.

Extraction LLM: phi-4  or  llama-3.2  (via local Ollama). Used only during the ingestion

phase to extract graph entities.

Plugin Server: Python MCP SDK. Lightweight wrapper to expose the databases to GitHub

Copilot/Cursor.

Phase 2: Data Acquisition & Normalization

Microsoft Docs are massive. To keep the footprint light, you must target specific repositories

rather than scraping the web.

1.  Target Repositories: Clone specific domains using Git.

https://github.com/MicrosoftDocs/azure-docs

https://github.com/dotnet/docs

2.  Normalization Script: Write a Python script to traverse the cloned directories.

Strip YAML: Remove the Microsoft-specific YAML frontmatter at the top of each  .md

file, keeping only  title ,  ms.date , and  description .

Strip UI Tags: Remove UI-specific markdown extensions (e.g.,  ::: zone ,  [!NOTE] ).

Filter Locales: Ensure only  en-us  (or your preferred locale) is kept to prevent

duplicate context.

Phase 3: The GraphRAG Extraction Pipeline (Ingestion)

https://gemini.google.com/app/bf3b2695f541bba4?utm_source=app_launcher&utm_medium=owned&utm_campaign=base_all

1/6

3/8/26, 9:33 PM

Google Gemini

This is a one-time (or cron-based) offline process to build the databases.

Step A: Semantic Chunking

Parse the cleaned markdown and chunk it based on Markdown Headers (H2, H3).

Rule: A chunk should be a self-contained concept (e.g., "How to authenticate with Azure

AD").

Metadata attached to every chunk: Source URL, Document Title, Parent Header.

Step B: Graph Extraction (The LLM Pass)

Pass each chunk to a local LLM with a strict JSON schema prompt to extract nodes and edges.

Nodes: Service  (e.g., Azure Blob Storage),  SDK_Class  (e.g., BlobClient),  Concept  (e.g.,

RBAC).

Edges: IMPLEMENTS ,  REQUIRES_PERMISSION ,  PART_OF .

Step C: Database Loading & Correlation

To link the graph and vector databases without a unified system, we use a shared  chunk_id

(UUID).

1.  Embed & Load to LanceDB: Store the text, metadata, and the vector embedding. The

primary key is  chunk_id .

2.  Load to Kùzu: Insert the extracted Nodes and Edges. Crucially, attach the  chunk_id  as a

property to the Nodes so Kùzu knows exactly where the text lives in LanceDB.

Phase 4: Database Schema Design

Kùzu Property Graph Schema (Cypher)

// Define Node Tables
CREATE NODE TABLE Service (name STRING, description STRING, PRIMARY KEY (name));
CREATE NODE TABLE SDK_Class (name STRING, language STRING, chunk_id STRING, PRIMA
CREATE NODE TABLE CodeSnippet (chunk_id STRING, language STRING, PRIMARY KEY (chu

// Define Edge Tables
CREATE REL TABLE CONTAINS (FROM Service TO SDK_Class);
CREATE REL TABLE REQUIRES_CONFIG (FROM SDK_Class TO Service);
CREATE REL TABLE HAS_EXAMPLE (FROM SDK_Class TO CodeSnippet);

https://gemini.google.com/app/bf3b2695f541bba4?utm_source=app_launcher&utm_medium=owned&utm_campaign=base_all

2/6

3/8/26, 9:33 PM

Google Gemini

LanceDB Schema (PyArrow)

schema = pa.schema([
    pa.field("chunk_id", pa.string()),          # Correlates to Kùzu
    pa.field("vector", pa.list_(pa.float32(), 768)), # Nomic embedding
    pa.field("text", pa.string()),              # The actual markdown content
    pa.field("document_title", pa.string()),
    pa.field("source_url", pa.string())
])

Phase 5: MCP Plugin Implementation

Create a local Python MCP server ( ms_docs_mcp.py ) that loads the local  .lance  and  .kuzu

directories.

Tool 1:  search_ms_docs_semantic

Description: "Use this to find specific code examples, tutorials, or explanations within

Microsoft and Azure documentation."

Action: Embeds the user's query, performs a vector similarity search in LanceDB, and

returns the top 3 markdown chunks.

Tool 2:  explore_ms_architecture

Description: "Use this to understand how different Azure services connect, or to find out

which SDK classes belong to which services before writing code."

Action: Takes an entity name (e.g., "Event Grid") and executes a Kùzu Cypher query:

MATCH (s:Service {name: 'Event Grid'})-[:CONTAINS]->(sdk:SDK_Class)-[:HAS_EXA
RETURN sdk.name, code.chunk_id

Resolution: The server takes the returned  chunk_id s, fetches the exact text from

LanceDB, and returns it to the agent.
