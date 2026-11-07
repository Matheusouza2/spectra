<!-- Context: project-intelligence/technical | Priority: critical | Version: 1.0 | Updated: 2026-05-31 -->

# Technical Domain

**Purpose**: Tech stack, architecture, and development patterns for racon-desktop.
**Last Updated**: 2026-05-31

## Quick Reference
**Update Triggers**: Tech stack changes | New patterns | Architecture decisions
**Audience**: Developers, AI agents

## Primary Stack
| Layer | Technology | Version | Rationale |
|-------|-----------|---------|-----------|
| Desktop Shell | Tauri v2 | ^2.11 | Native desktop via system webview |
| Frontend | React + Vite | ^18.2 / ^4.0 | Component-based UI, fast HMR |
| Language (FE) | JavaScript (JSX) | — | No TypeScript — all .jsx files |
| Language (BE) | Rust | edition 2021 | Safe systems programming |
| Graph Viz | @antv/g6 | ^5.1 | Force-directed graph rendering |
| IPC | @tauri-apps/api/core | ^2.11 | invoke() bridge to Rust commands |
| Dialog | @tauri-apps/plugin-dialog | ^2 | Native file picker for .pnt files |
| Core Lib | racon (local) | path = "../.." | Petri net analysis algorithms |

## Code Patterns

### Tauri IPC Command (Frontend → Rust)
```js
// src/services/tauri-api.js — service layer wraps invoke()
import { invoke } from '@tauri-apps/api/core';

export const analyzePnt = (filePath) => invoke('analyze_pnt', { filePath });
export const checkCtlFormula = (filePath, formula) =>
  invoke('check_ctl_formula', { filePath, formula });
export const tgFireTransition = (filePath, marking, transition) =>
  invoke('tg_fire_transition', { filePath, marking, transition });
```

### Tauri Command Handler (Rust Backend)
```rust
// src-tauri/src/main.rs — thin wrapper, no algorithmic logic
#[tauri::command]
fn analyze_pnt(file_path: String) -> Result<AnalysisResult, String> {
    let content = fs::read_to_string(&file_path).map_err(|e| e.to_string())?;
    let net = PntParser::parse_str(&content).map_err(|e| e.to_string())?;
    // All analysis imports from racon::*
    let ct = net.build_coverage_tree_limit(100_000);
    let structural = check_structural(&net);
    // ...
}
```

### React Component
```jsx
// Functional component, CSS module naming, error boundaries on tabs
import React, { useState, useEffect, useRef, useCallback } from 'react';
import './ComponentName.css';

const ComponentName = ({ prop1, prop2 }) => {
  const [state, setState] = useState(null);
  const containerRef = useRef(null);

  useEffect(() => {
    // Init side effects (G6 graph, etc.)
    return () => { /* cleanup */ };
  }, []);

  return <div ref={containerRef}>{/* JSX */}</div>;
};

export default ComponentName;
```

### File Upload + Analysis Workflow
```jsx
// src/components/FileUpload.jsx
import { open } from '@tauri-apps/plugin-dialog';
import { analyzePnt } from '../services/tauri-api';
import { useToast } from './Toast';

const FileUpload = ({ onAnalysisComplete, onFileSelect }) => {
  // Native file dialog → analyze_pnt IPC → callback with results
  // Drag-and-drop support for .pnt files
  // Loading spinner during analysis
};
```

## Naming Conventions
| Type | Convention | Example |
|------|-----------|---------|
| Component files | PascalCase.jsx | PetriNetGraph.jsx |
| Utility/service files | camelCase.js | tauri-api.js, useTauriCommand.js |
| CSS files | PascalCase.css | PetriNetGraph.css |
| Components (export) | PascalCase | export default PetriNetGraph |
| Functions | camelCase | analyzePnt, handleUpload |
| Rust Tauri commands | snake_case | analyze_pnt, get_coverage_tree |
| Rust types | PascalCase | AnalysisResult, GraphNode |

## Code Standards
- **JSX only** — no TypeScript, no .tsx files
- **Functional components** with hooks (useState, useEffect, useRef, useCallback)
- **Component + CSS pair** — every component has a matching .css file
- **CSS custom properties** for theming via `[data-theme]` (light/dark)
- **Error boundaries** wrap each tab pane for isolated error handling
- **Toast notifications** for user feedback (success/error/warning)
- **Service abstraction** — IPC calls go through `src/services/tauri-api.js`
- **Rust backend is thin** — no algorithmic code in main.rs, delegate to `racon::*`
- **No linter, formatter, or test scripts configured**

## Security Requirements
- Desktop local app — no authentication, no network requests, no user data storage
- File input restricted to `.pnt` via dialog filter and drop-zone validation
- Rust commands return `Result<_, String>` — no unwrap/panic in IPC handlers
- Frontend wraps all IPC calls in try/catch with toast error display
- Tauri capabilities: `core:default` + `dialog:default` only
- CSP is `null` (no content security policy — permissive, Tauri default)

## 📂 Codebase References
- **IPC service layer**: `src/services/tauri-api.js` — all 10 invoke() wrappers
- **Rust command handlers**: `src-tauri/src/main.rs` — all 10 #[tauri::command] functions
- **Component examples**: `src/components/PetriNetGraph.jsx` (G6), `FileUpload.jsx` (dialog + IPC)
- **Theme system**: `src/index.css` (:root + [data-theme='dark']), `src/contexts/ThemeContext.jsx`
- **Racon core library**: `../src/` — parent directory, path dependency
- **Config**: `package.json`, `vite.config.js`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`

## Related Files
- AGENTS.md (project rules)
- `../AGENTS.md` (racon core library rules)
