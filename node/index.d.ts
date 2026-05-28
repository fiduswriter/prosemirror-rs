/**
 * Node.js bindings for prosemirror-rs.
 *
 * Provides a memory- and CPU-efficient interface to ProseMirror's document
 * model and transform pipeline.  Document state lives entirely in Rust; only
 * JSON strings cross the JavaScript/Rust boundary.
 *
 * Schema caching:
 * - The first `new Editor(schemaJson, ...)` call parses the schema and stores
 *   it in a global Rust cache keyed by the exact schema-JSON string.
 * - All subsequent `Editor` constructions with the same string reuse the
 *   cached schema at the cost of a single `Arc` clone.
 */
export declare class Editor {
  /**
   * Create a new Editor.
   *
   * The parsed schema is cached inside Rust (keyed by the exact
   * `schemaJson` string), so repeated construction with the same schema
   * only parses it once.
   *
   * @param schemaJson ProseMirror schema specification as a JSON string.
   * @param docJson Initial document state as a JSON string.
   * @throws {Error} If either string is not valid JSON, or the schema /
   *   document does not conform to the ProseMirror spec.
   */
  constructor(schemaJson: string, docJson: string);

  /**
   * Apply a single step to the document.
   *
   * @param stepJson The step as a JSON string.
   * @returns `true` if applied successfully, `false` if the step could not
   *   be applied (document is left unchanged).
   * @throws {Error} If `stepJson` is not valid JSON or not a recognised step type.
   */
  applyStep(stepJson: string): boolean;

  /**
   * Apply a batch of steps supplied as a single JSON array string, atomically.
   *
   * Preferred method when steps arrive from a network client: the string is
   * passed directly to Rust and parsed there, so nothing touches JS's JSON
   * machinery.
   *
   * All steps are parsed before any are applied, so a malformed array throws
   * without mutating the document.
   *
   * The batch is fully atomic: if any step fails to apply the document is
   * rolled back to its state before the call, leaving it completely unchanged.
   * The version counter is likewise rolled back.
   *
   * @param stepsJson A JSON array of step objects, e.g.
   *   `'[{"stepType":"replace",...},...]'`.
   * @returns `true` if every step applied successfully; `false` if any step
   *   failed (document and version are rolled back entirely).
   * @throws {Error} If `stepsJson` is not a valid JSON array of steps.
   */
  applyStepsJson(stepsJson: string): boolean;

  /**
   * Apply a batch of steps from a JS array of JSON strings, atomically.
   *
   * Use this when steps are constructed or modified in JS.  For steps arriving
   * directly from a network client prefer `applyStepsJson` to avoid unnecessary
   * JS-level JSON parsing.
   *
   * All steps are parsed before any are applied, so a bad JSON string throws
   * without mutating the document.
   *
   * The batch is fully atomic: if any step fails to apply the document is
   * rolled back to its state before the call, leaving it completely unchanged.
   * The version counter is likewise rolled back.
   *
   * @param steps An array where each element is a JSON string for one step.
   * @returns `true` if every step applied successfully; `false` if any step
   *   failed (document and version are rolled back entirely).
   * @throws {Error} If any element is not valid step JSON.
   */
  applySteps(steps: string[]): boolean;

  /**
   * Reset the document to a new state, reusing the already-parsed schema.
   *
   * More efficient than constructing a brand-new `Editor` when you need to
   * restore a snapshot (e.g. after an unrecoverable conflict), because the
   * schema is never re-parsed — only the document JSON is processed.
   * The version counter is reset to zero.
   *
   * @param docJson The replacement document as a JSON string.
   * @throws {Error} If `docJson` is not valid JSON or does not conform to
   *   the schema.
   */
  reset(docJson: string): void;

  /**
   * Serialize the current document to a JSON string.
   *
   * Serialization happens entirely in Rust; only the resulting string is
   * passed to JavaScript.  Suitable for saving directly to a database without
   * creating any intermediate JS objects.
   *
   * When `skipDefaults` is `true`, attributes whose value matches the
   * schema-defined default are omitted from the output ("mini" JSON).
   *
   * @param skipDefaults If true, omit attributes that have default values.
   * @returns The current document as a compact JSON string.
   */
  docJson(skipDefaults?: boolean): string;

  /**
   * Number of steps successfully applied since construction or last `reset()`.
   *
   * Use as a document version counter in collaborative-editing protocols.
   */
  readonly version: number;
}
