// TypeScript declarations for prosemirror-rs.
//
// This package replaces both prosemirror-model and prosemirror-transform.
// Re-exports everything from model, transform, and DOM sub-declarations.

export * from "./model"
export * from "./transform"
export { DOMOutputSpec, DOMSerializer, DOMParser, ParseRule, TagParseRule, StyleParseRule, GenericParseRule, ParseOptions } from "./dom"

/**
 * A stateful ProseMirror document editor backed by Rust.
 *
 * The schema and document state live entirely in Rust memory. Only JSON
 * strings cross the JavaScript/Rust boundary.
 */
export declare class Editor {
  constructor(schemaJson: string, docJson: string)
  applyStep(stepJson: string): boolean
  applyStepsJson(stepsJson: string): boolean
  applySteps(steps: string[]): boolean
  reset(docJson: string): void
  docJson(skipDefaults?: boolean): string
  readonly version: number
}
