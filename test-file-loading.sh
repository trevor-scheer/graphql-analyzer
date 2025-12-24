#!/bin/bash
# Simple test to verify file loading

echo "Testing file loading implementation..."
echo ""
echo "Expected log messages in LSP output:"
echo "1. 'GraphQL config found, loading files...'"
echo "2. Multiple 'Loaded schema file: ...' messages"
echo "3. Multiple 'Loaded document file: ...' messages"  
echo "4. 'Finished loading all project files into AnalysisHost'"
echo ""
echo "Please check your VSCode Output panel (GraphQL LSP) for these messages."
echo ""
echo "If you don't see these messages, the files aren't being loaded."
