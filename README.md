# Psynapse

[![TypeScript](https://img.shields.io/badge/typescript-5.4-blue?logo=typescript)](https://www.typescriptlang.org)
[![Next.js](https://img.shields.io/badge/next.js-16-black?logo=next.js)](https://nextjs.org)
[![Docker](https://img.shields.io/badge/docker-multi--stage-2496ED?logo=docker)](https://docker.com)

> LLM-powered node-based platform for document Q&A, summarization, and knowledge management. Upload PDFs, scrape web pages, fetch YouTube transcripts — then ask questions in natural language and get streaming answers with source citations.

## Architecture

Monorepo with:
- `apps/web` - Next.js frontend
- `apps/api` - Rust/Axum backend with PostgreSQL + pgvector

## Getting Started

### Prerequisites
- Node.js 20+
- Rust toolchain
- PostgreSQL with pgvector extension

### Development
```bash
# Start frontend
npm run dev

# Start backend
cd apps/api && cargo run
```
```