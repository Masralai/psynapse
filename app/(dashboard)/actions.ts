"use server";

import { requireAuth } from "@/modules/auth";
import { nodeService } from "@/modules/node";
import { summarize, type SummaryType } from "@/modules/rag";
import { revalidatePath } from "next/cache";
import { mkdir, writeFile } from "fs/promises";

export async function createNode(name: string) {
  const { userId } = await requireAuth();
  const node = await nodeService.createNode(userId, name);
  revalidatePath("/");
  return node;
}

export async function listNodes() {
  const { userId } = await requireAuth();
  return nodeService.listNodes(userId);
}

export async function listNodesWithStats() {
  const { userId } = await requireAuth();
  return nodeService.listNodesWithStats(userId);
}

export async function renameNode(id: string, name: string) {
  const { userId } = await requireAuth();
  await nodeService.assertNodeOwner(id, userId);
  const node = await nodeService.renameNode(id, name);
  revalidatePath("/");
  return node;
}

export async function deleteNode(id: string) {
  const { userId } = await requireAuth();
  await nodeService.assertNodeOwner(id, userId);
  await nodeService.deleteNode(id);
  revalidatePath("/");
}

export async function addSource(
  nodeId: string,
  type: "pdf" | "docx" | "txt" | "csv" | "md" | "html" | "xlsx" | "web" | "youtube",
  formData: FormData,
) {
  const { userId } = await requireAuth();
  await nodeService.assertNodeOwner(nodeId, userId);

  const file = formData.get("file") as File | null;
  const url = formData.get("url") as string | null;
  const name = formData.get("name") as string | null;

  const source = await nodeService.addSource({
    nodeId,
    type,
    name: name || file?.name || url || "unknown",
    status: "pending",
    metadata: url ? { url } : {},
  });

  if (file) {
    const dir = `/tmp/psynapse/${source.id}`;
    await mkdir(dir, { recursive: true });
    await writeFile(`${dir}/${file.name}`, Buffer.from(await file.arrayBuffer()));
  }

  revalidatePath(`/nodes/${nodeId}`);
  return { sourceId: source.id };
}

export async function removeSource(sourceId: string, nodeId: string) {
  const { userId } = await requireAuth();
  await nodeService.assertNodeOwner(nodeId, userId);
  await nodeService.removeSource(sourceId);
  revalidatePath(`/nodes/${nodeId}`);
}

export async function setSourceEnabled(sourceId: string, nodeId: string, enabled: boolean) {
  const { userId } = await requireAuth();
  await nodeService.assertNodeOwner(nodeId, userId);
  await nodeService.updateSource(sourceId, { enabled });
  revalidatePath(`/nodes/${nodeId}`);
}

export async function listSources(nodeId: string) {
  const { userId } = await requireAuth();
  await nodeService.assertNodeOwner(nodeId, userId);
  return nodeService.listSources(nodeId);
}

export async function listMessages(nodeId: string) {
  const { userId } = await requireAuth();
  await nodeService.assertNodeOwner(nodeId, userId);
  return nodeService.listMessages(nodeId);
}

export async function getSourceContent(sourceId: string) {
  const { userId } = await requireAuth();
  const source = await nodeService.getSource(sourceId);
  if (!source) throw new Error("Source not found");
  await nodeService.assertNodeOwner(source.nodeId, userId);
  return nodeService.getSourceContent(sourceId);
}

export async function generateSummary(nodeId: string, type: SummaryType) {
  const { userId } = await requireAuth();
  await nodeService.assertNodeOwner(nodeId, userId);
  return summarize(nodeId, type);
}

export async function listArtifacts(nodeId: string) {
  const { userId } = await requireAuth();
  await nodeService.assertNodeOwner(nodeId, userId);
  return nodeService.listArtifacts(nodeId);
}

export async function getLatestArtifact(nodeId: string, type: SummaryType) {
  const { userId } = await requireAuth();
  await nodeService.assertNodeOwner(nodeId, userId);
  return nodeService.latestArtifact(nodeId, type);
}
