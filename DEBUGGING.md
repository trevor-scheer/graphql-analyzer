# Debugging the GraphQL LSP Extension

## Step 1: Verify the LSP Server Works

The LSP server binary should exist and respond to requests:

```bash
# Check the binary exists
ls -lh target/debug/graphql-lsp

# Test it responds (should see initialization response)
./test-lsp.sh
```

You should see a JSON response with `"result":{"capabilities":{...}}` - this means the server is working!

## Step 2: Launch the Extension with Debugging

1. **Open the extension directory in VS Code:**
   ```bash
   cd editors/vscode
   code .
   ```

2. **Open the Debug panel** (Ctrl+Shift+D / Cmd+Shift+D)

3. **Press F5** to launch "Extension" configuration

4. **Look for the Output panel** in the Extension Development Host window
   - It should automatically show "GraphQL LSP Debug" channel
   - You should see messages like:
     ```
     GraphQL LSP extension activating...
     LSP server command: /path/to/graphql-lsp
     Creating language client...
     Starting language client...
     Extension activated!
     Language client started successfully!
     ```

## Step 3: Check File Association

Open a `.graphql` file in the Extension Development Host and check:

1. **Look at the bottom-right corner** of VS Code - it should say "GraphQL" as the language mode
   - If it says "Plain Text", click on it and select "GraphQL" from the dropdown

2. **Check the Output panel** - you should see logs when opening a GraphQL file

## Step 4: Manual Language Mode Selection

If the extension isn't auto-detecting `.graphql` files:

1. Open `test-workspace/example.graphql`
2. Click the language mode indicator in the bottom-right (might say "Plain Text")
3. Type "graphql" and select "GraphQL" from the dropdown
4. The extension should activate and start validating

## Common Issues

### Issue 1: "Language client failed to start"

**Check:** The GRAPHQL_LSP_PATH environment variable

In `.vscode/launch.json`, verify the path:
```json
"env": {
  "GRAPHQL_LSP_PATH": "${workspaceFolder}/../../target/debug/graphql-lsp"
}
```

Try using an absolute path:
```json
"GRAPHQL_LSP_PATH": "/Users/trevor/Repositories/graphql-lsp/target/debug/graphql-lsp"
```

### Issue 2: No output in the Output panel

The extension might not be activating. Check:

1. **Developer Tools**: In the Extension Development Host, open Help > Toggle Developer Tools
2. Look for any JavaScript errors in the Console tab
3. Check if `activate()` was called

### Issue 3: Extension not activating at all

VS Code might not recognize `.graphql` files. Try:

1. Open Command Palette (Cmd+Shift+P / Ctrl+Shift+P)
2. Type "Change Language Mode"
3. Select "GraphQL"

Or add this to the Extension Development Host's settings (File > Preferences > Settings):
```json
{
  "files.associations": {
    "*.graphql": "graphql",
    "*.gql": "graphql"
  }
}
```

### Issue 4: Server starts but no diagnostics appear

Check the LSP server logs:

1. Set `RUST_LOG=debug` in the launch configuration
2. The logs will appear in the "GraphQL LSP Debug" output channel
3. Look for "Document opened" or "Document changed" messages

## Debugging Checklist

- [ ] LSP binary exists: `ls target/debug/graphql-lsp`
- [ ] LSP binary responds: `./test-lsp.sh` shows initialization response
- [ ] Extension compiled: `cd editors/vscode && npm run compile`
- [ ] Output panel shows "GraphQL LSP Debug" channel
- [ ] Output shows "Extension activated!"
- [ ] Output shows "Language client started successfully!"
- [ ] File is recognized as GraphQL (bottom-right shows "GraphQL")
- [ ] Opening a `.graphql` file shows "Document opened" in logs

## Testing with the Debugger

You can set breakpoints in the TypeScript extension code:

1. Open `editors/vscode/src/extension.ts`
2. Set a breakpoint in the `activate()` function
3. Press F5
4. When you open a GraphQL file, the debugger should pause at your breakpoint

## Next Steps

Once you see "Language client started successfully!" in the output:

1. Open `test-workspace/example.graphql`
2. You should see validation errors for `invalidField`, `email`, and `anotherInvalidField`
3. Errors might appear at line 0 (we need to improve location parsing)

If you're still having issues, please share:
- The contents of the "GraphQL LSP Debug" output panel
- Any errors from the Developer Tools Console
- The language mode shown in the bottom-right corner
