import { NodeRepo } from "./repository";
import type { Citation, SourceInput } from "./types";

export type { Node, Source, Chunk, ChatMessage, SourceInput, Citation, Artifact } from "./types";

const repo = new NodeRepo();

export const nodeService = {
  createNode: (userId: string, name: string) => repo.createNode(userId, name),
  listNodes: (userId: string) => repo.listNodes(userId),
  listNodesWithStats: (userId: string) => repo.listNodesWithStats(userId),
  getNode: (id: string) => repo.getNode(id),
  assertNodeOwner: (nodeId: string, userId: string) => repo.assertNodeOwner(nodeId, userId),
  renameNode: (id: string, name: string) => repo.renameNode(id, name),
  deleteNode: (id: string) => repo.deleteNode(id),

  addSource: (input: SourceInput) => repo.addSource(input),
  getSource: (id: string) => repo.getSource(id),
  getSourcesByIds: (ids: string[]) => repo.getSourcesByIds(ids),
  removeSource: (id: string) => repo.removeSource(id),
  listSources: (nodeId: string) => repo.listSources(nodeId),
  getSourceContent: (id: string) => repo.getSourceContent(id),
  updateSource: (
    id: string,
    data: {
      status?: string;
      rawText?: string;
      metadata?: Record<string, unknown>;
      progress?: Record<string, unknown> | null;
      enabled?: boolean;
    },
  ) => repo.updateSource(id, data),
  searchSources: (nodeId: string, query: string) => repo.searchSources(nodeId, query),

  addChunks: (sourceId: string, nodeId: string, entries: { index: number; content: string; embedding?: number[] }[]) =>
    repo.addChunks(sourceId, nodeId, entries),
  deleteChunksBySource: (sourceId: string) => repo.deleteChunksBySource(sourceId),
  deleteChunksByNode: (nodeId: string) => repo.deleteChunksByNode(nodeId),
  searchChunks: (nodeId: string, queryEmbedding: number[], topK?: number) =>
    repo.searchChunks(nodeId, queryEmbedding, topK),

  addMessage: (
    nodeId: string,
    role: "user" | "assistant",
    content: string,
    sources?: string[],
    citations?: Citation[],
  ) => repo.addMessage(nodeId, role, content, sources, citations),
  listMessages: (nodeId: string) => repo.listMessages(nodeId),

  addArtifact: (nodeId: string, type: "study-guide" | "faq", content: string) =>
    repo.addArtifact(nodeId, type, content),
  listArtifacts: (nodeId: string) => repo.listArtifacts(nodeId),
  latestArtifact: (nodeId: string, type: "study-guide" | "faq") =>
    repo.latestArtifact(nodeId, type),
};
