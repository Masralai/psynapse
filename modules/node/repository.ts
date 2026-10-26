import { db } from "@/lib/db";
import { nodes, sources, chatMessages, chunks, artifacts } from "@/lib/db/schema";
import { eq, and, like, inArray, sql, desc } from "drizzle-orm";
import type { Node, Source, ChatMessage, SourceInput, Chunk, Citation, Artifact } from "./types";

function toSource(row: any): Source {
  return {
    ...row,
    enabled: row.enabled !== false,
    metadata: (row.metadata || {}) as Record<string, unknown>,
  };
}

function toMessage(row: any): ChatMessage {
  return {
    ...row,
    sources: row.sources || [],
    citations: (row.citations || []) as Citation[],
  };
}

export class NodeRepo {
  async createNode(userId: string, name: string): Promise<Node> {
    const [n] = await db.insert(nodes).values({ userId, name }).returning();
    return n;
  }

  async listNodes(userId: string): Promise<Node[]> {
    return db.select().from(nodes).where(eq(nodes.userId, userId));
  }

  // ponytail: single join-group, paginate at 50 if needed
  async listNodesWithStats(userId: string): Promise<(Node & { sourceCount: number; readyCount: number })[]> {
    const rows = await db
      .select({
        id: nodes.id,
        userId: nodes.userId,
        name: nodes.name,
        createdAt: nodes.createdAt,
        updatedAt: nodes.updatedAt,
        sourceCount: sql<number>`count(${sources.id})`.mapWith(Number),
        readyCount: sql<number>`count(*) filter (where ${sources.status} = 'ready')`.mapWith(Number),
      })
      .from(nodes)
      .leftJoin(sources, eq(sources.nodeId, nodes.id))
      .where(eq(nodes.userId, userId))
      .groupBy(nodes.id)
      .orderBy(desc(nodes.updatedAt));
    return rows as any;
  }

  async getNode(id: string): Promise<Node | null> {
    const [n] = await db.select().from(nodes).where(eq(nodes.id, id));
    return n ?? null;
  }

  async assertNodeOwner(nodeId: string, userId: string): Promise<Node> {
    const node = await this.getNode(nodeId);
    if (!node || node.userId !== userId) {
      throw new Error("Forbidden");
    }
    return node;
  }

  async renameNode(id: string, name: string): Promise<Node> {
    const [n] = await db
      .update(nodes)
      .set({ name, updatedAt: new Date() })
      .where(eq(nodes.id, id))
      .returning();
    return n;
  }

  async deleteNode(id: string): Promise<void> {
    await db.delete(nodes).where(eq(nodes.id, id));
  }

  async addSource(input: SourceInput): Promise<Source> {
    const [s] = await db
      .insert(sources)
      .values({
        nodeId: input.nodeId,
        type: input.type,
        name: input.name || "unknown",
        status: (input.status as any) || "pending",
        rawText: input.rawText,
        metadata: (input.metadata || {}) as any,
      })
      .returning();
    return toSource(s);
  }

  async removeSource(id: string): Promise<void> {
    await db.delete(sources).where(eq(sources.id, id));
  }

  async listSources(nodeId: string): Promise<Source[]> {
    const rows = await db.select().from(sources).where(eq(sources.nodeId, nodeId));
    return rows.map(toSource);
  }

  async getSourcesByIds(ids: string[]): Promise<Source[]> {
    if (ids.length === 0) return [];
    const rows = await db.select().from(sources).where(inArray(sources.id, ids));
    return rows.map(toSource);
  }

  async getSource(id: string): Promise<Source | null> {
    const [s] = await db.select().from(sources).where(eq(sources.id, id));
    return s ? toSource(s) : null;
  }

  async getSourceContent(id: string): Promise<string> {
    const [s] = await db
      .select({ rawText: sources.rawText })
      .from(sources)
      .where(eq(sources.id, id));
    return s?.rawText || "";
  }

  async updateSource(
    id: string,
    data: {
      status?: string;
      rawText?: string;
      metadata?: Record<string, unknown>;
      progress?: Record<string, unknown> | null;
      enabled?: boolean;
    },
  ): Promise<void> {
    await db
      .update(sources)
      .set({
        ...(data.status ? { status: data.status as any } : {}),
        ...(data.rawText !== undefined ? { rawText: data.rawText } : {}),
        ...(data.metadata ? { metadata: data.metadata as any } : {}),
        ...(data.progress !== undefined ? { progress: data.progress as any } : {}),
        ...(data.enabled !== undefined ? { enabled: data.enabled } : {}),
      })
      .where(eq(sources.id, id));
  }

  async searchSources(nodeId: string, query: string): Promise<Source[]> {
    const rows = await db
      .select()
      .from(sources)
      .where(and(eq(sources.nodeId, nodeId), like(sources.name, `%${query}%`)));
    return rows.map(toSource);
  }

  async addMessage(
    nodeId: string,
    role: "user" | "assistant",
    content: string,
    sourcesList: string[] = [],
    citationsList: Citation[] = [],
  ): Promise<ChatMessage> {
    const [m] = await db
      .insert(chatMessages)
      .values({
        nodeId,
        role,
        content,
        sources: sourcesList,
        citations: citationsList,
      })
      .returning();
    return toMessage(m);
  }

  async addChunks(
    sourceId: string,
    nodeId: string,
    entries: { index: number; content: string; embedding?: number[] }[],
  ): Promise<void> {
    if (entries.length === 0) return;

    for (const e of entries) {
      await db.insert(chunks).values({
        sourceId,
        nodeId,
        index: e.index,
        content: e.content,
        embedding: e.embedding ? sql`${`[${e.embedding.join(",")}]`}::vector` : null,
      });
    }
  }

  async deleteChunksBySource(sourceId: string): Promise<void> {
    await db.delete(chunks).where(eq(chunks.sourceId, sourceId));
  }

  async deleteChunksByNode(nodeId: string): Promise<void> {
    await db.delete(chunks).where(eq(chunks.nodeId, nodeId));
  }

  async searchChunks(
    nodeId: string,
    queryEmbedding: number[],
    topK: number = 6,
  ): Promise<Chunk[]> {
    const embeddingStr = `[${queryEmbedding.join(",")}]`;

    const rows = await db
      .select({
        id: chunks.id,
        sourceId: chunks.sourceId,
        nodeId: chunks.nodeId,
        index: chunks.index,
        content: chunks.content,
        embedding: chunks.embedding,
        createdAt: chunks.createdAt,
        sourceName: sources.name,
      })
      .from(chunks)
      .innerJoin(sources, eq(chunks.sourceId, sources.id))
      .where(and(eq(chunks.nodeId, nodeId), eq(sources.enabled, true)))
      .orderBy(sql`${chunks.embedding} <=> ${embeddingStr}::vector`)
      .limit(topK);

    return rows.map((r: any) => ({
      ...r,
      embedding: r.embedding ? JSON.parse(JSON.stringify(r.embedding)) : undefined,
    }));
  }

  async listMessages(nodeId: string): Promise<ChatMessage[]> {
    const rows = await db
      .select()
      .from(chatMessages)
      .where(eq(chatMessages.nodeId, nodeId))
      .orderBy(chatMessages.createdAt);
    return rows.map(toMessage);
  }

  async addArtifact(nodeId: string, type: "study-guide" | "faq", content: string): Promise<Artifact> {
    const [a] = await db
      .insert(artifacts)
      .values({ nodeId, type, content })
      .returning();
    return a as Artifact;
  }

  async listArtifacts(nodeId: string): Promise<Artifact[]> {
    return db
      .select()
      .from(artifacts)
      .where(eq(artifacts.nodeId, nodeId))
      .orderBy(desc(artifacts.createdAt)) as Promise<Artifact[]>;
  }

  async latestArtifact(nodeId: string, type: "study-guide" | "faq"): Promise<Artifact | null> {
    const [a] = await db
      .select()
      .from(artifacts)
      .where(and(eq(artifacts.nodeId, nodeId), eq(artifacts.type, type)))
      .orderBy(desc(artifacts.createdAt))
      .limit(1);
    return (a as Artifact) ?? null;
  }
}
