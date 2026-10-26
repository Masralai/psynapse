import {
  FileText,
  FileCode,
  Table,
  Code,
  Link,
  Video,
  File as FileIcon,
  type LucideIcon,
} from "lucide-react";

const iconMap: Record<string, LucideIcon> = {
  pdf: FileText,
  docx: FileText,
  txt: FileText,
  csv: Table,
  md: FileCode,
  html: Code,
  htm: Code,
  xlsx: Table,
  xls: Table,
  web: Link,
  youtube: Video,
};

export function SourceIcon({ type, size = 14 }: { type: string; size?: number }) {
  const Icon = iconMap[type] || FileIcon;
  return <Icon size={size} className="text-champagne/70 shrink-0" />;
}

export function getSourceIcon(type: string): LucideIcon {
  return iconMap[type] || FileIcon;
}
