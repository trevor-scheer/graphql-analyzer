import * as monaco from "monaco-editor";

monaco.languages.register({ id: "graphql", extensions: [".graphql", ".gql"] });

const schema = monaco.editor.create(document.getElementById("schema")!, {
  language: "graphql",
  value: "# schema goes here\n",
  automaticLayout: true,
});
const doc = monaco.editor.create(document.getElementById("doc")!, {
  language: "graphql",
  value: "# query goes here\n",
  automaticLayout: true,
});

console.log("editors mounted", schema.getModel(), doc.getModel());
