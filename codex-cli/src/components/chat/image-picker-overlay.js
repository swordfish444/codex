// Thin re‑export shim so test files that import `image-picker-overlay.js`
// continue to work even though the real component is authored in TypeScript.
//
// We deliberately keep this file in plain JavaScript so Node can resolve it
// without the “.tsx” extension when running under ts-node/esm in the test
// environment.

export { default } from "./image-picker-overlay.tsx";
