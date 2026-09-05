import { readFileSync, statSync } from "node:fs";
import { extname } from "node:path";
import glob from "fast-glob";
import { isDocumentString } from "@graphql-tools/utils";
import type { Source } from "@graphql-tools/utils";
import type { GraphQLProjectConfig } from "graphql-config";

type Snapshot = { sources: Source[]; key?: string };
const snapshots = new Map<string, Snapshot>();

function staticKey(project: GraphQLProjectConfig, kind: "schema" | "documents") {
  const pointer = project[kind];
  const pointers = Array.isArray(pointer) ? pointer : [pointer];
  if (pointers.some((value) => typeof value !== "string")) return;
  const strings = pointers as string[];
  const patterns = strings.filter((value) => !isDocumentString(value));
  if (patterns.some((value) => /^[a-z][a-z\d+.-]*:/iu.test(value) && !/^[a-z]:[/\\]/iu.test(value)))
    return;
  const files = glob
    .sync(patterns, { cwd: project.dirpath, absolute: true, onlyFiles: true })
    .sort();
  if (files.some((file) => ![".graphql", ".gql", ".json"].includes(extname(file)))) return;
  const stamps = files.map((file) => {
    const stat = statSync(file, { bigint: true });
    return [file, String(stat.mtimeNs), String(stat.ctimeNs), String(stat.size), String(stat.ino)];
  });
  const extensions = JSON.stringify(project.extensions, (_key, value) => {
    if (typeof value === "function") throw new Error("Dynamic loader options cannot be cached");
    return value;
  });
  return {
    key: JSON.stringify([
      project.filepath,
      project.name,
      project.dirpath,
      extensions,
      kind,
      strings,
      stamps,
    ]),
    files,
  };
}

export function loadSources(
  project: GraphQLProjectConfig,
  kind: "schema" | "documents",
  load: () => Source[],
): Snapshot {
  let fingerprint: ReturnType<typeof staticKey>;
  try {
    fingerprint = staticKey(project, kind);
  } catch {
    /* Let the loader report inaccessible files. */
  }
  const cached = fingerprint && snapshots.get(fingerprint.key);
  if (cached) {
    snapshots.delete(fingerprint!.key);
    snapshots.set(fingerprint!.key, cached);
    return cached;
  }
  const sources = load();
  if (
    !fingerprint ||
    (kind === "schema" &&
      fingerprint.files.some((file) => /^\s*#\s*import\b/mu.test(readFileSync(file, "utf8"))))
  )
    return { sources };
  const snapshot = { sources, key: fingerprint.key };
  snapshots.set(fingerprint.key, snapshot);
  if (snapshots.size > 16) snapshots.delete(snapshots.keys().next().value!);
  return snapshot;
}
