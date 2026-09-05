import { Kind, visit } from "graphql";
import type { DocumentNode } from "graphql";
import type { FragmentSource, OperationSource, SiblingOperations } from "./types";

export function getSiblings(
  sources: { location: string; document: DocumentNode }[],
): SiblingOperations {
  const fragments: FragmentSource[] = [];
  const operations: OperationSource[] = [];
  for (const { location: filePath, document } of sources) {
    for (const definition of document.definitions) {
      if (definition.kind === Kind.FRAGMENT_DEFINITION)
        fragments.push({ filePath, document: definition });
      if (definition.kind === Kind.OPERATION_DEFINITION)
        operations.push({ filePath, document: definition });
    }
  }
  const getFragment = (name: string) =>
    fragments.filter((source) => source.document.name.value === name);
  return {
    available: sources.length > 0,
    getFragment,
    getFragments: () => fragments,
    getFragmentByType: (type) =>
      fragments.filter((source) => source.document.typeCondition.name.value === type),
    getOperations: () => operations,
    getOperation: (name) => operations.filter((source) => source.document.name?.value === name),
    getOperationByType: (type) => operations.filter((source) => source.document.operation === type),
    getFragmentsInUse(node, recursive = true) {
      const collected = new Map<string, FragmentSource["document"]>();
      function collect(current: Parameters<typeof visit>[0]) {
        visit(current, {
          FragmentSpread(spread) {
            const name = spread.name.value;
            const fragment = getFragment(name)[0];
            if (!fragment || collected.has(name)) return;
            collected.set(name, fragment.document);
            if (recursive) collect(fragment.document);
          },
        });
      }
      collect(node);
      return [...collected.values()];
    },
  };
}
