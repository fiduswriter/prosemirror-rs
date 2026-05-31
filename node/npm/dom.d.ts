// TypeScript declarations for prosemirror-rs DOM types.
// These are pure-JavaScript supplements vendored from prosemirror-model.

export type DOMNode = InstanceType<typeof window.Node>

export type DOMOutputSpec =
  | HTMLElement
  | { dom: HTMLElement; contentDOM?: HTMLElement }
  | readonly [string, ...any[]]

export class DOMSerializer {
  readonly nodes: { [node: string]: (node: any) => DOMOutputSpec }
  readonly marks: { [mark: string]: (mark: any, inline: boolean) => DOMOutputSpec }
  constructor(
    nodes: { [node: string]: (node: any) => DOMOutputSpec },
    marks: { [mark: string]: (mark: any, inline: boolean) => DOMOutputSpec }
  )
  serializeFragment(
    fragment: any,
    options?: { document?: Document },
    target?: HTMLElement | DocumentFragment
  ): HTMLElement | DocumentFragment
  serializeNode(node: any, options?: { document?: Document }): HTMLElement
  static renderSpec(
    doc: Document,
    structure: DOMOutputSpec,
    xmlNS?: string | null
  ): { dom: HTMLElement; contentDOM?: HTMLElement }
  static fromSchema(schema: any): DOMSerializer
  static nodesFromSchema(schema: any): { [node: string]: (node: any) => DOMOutputSpec }
  static marksFromSchema(
    schema: any
  ): { [mark: string]: (mark: any, inline: boolean) => DOMOutputSpec }
}

export interface ParseOptions {
  topNode?: any
  topMatch?: any
  preserveWhitespace?: boolean | "full"
  findPositions?: { node: any; offset: number; pos?: number }[] | null
  context?: string | null
  ruleFromNode?: (node: HTMLElement) => any
  topNodeType?: any
}

export interface GenericParseRule {
  tag?: string
  namespace?: string
  style?: string
  getContent?: (node: HTMLElement, schema: any) => any
  getAttrs?: (node: HTMLElement | string) => any | false | null
  priority?: number
  consuming?: boolean
  context?: string
  node?: string
  mark?: string
  ignore?: boolean
  clearMark?: (mark: any) => boolean
  contentElement?: string | ((node: HTMLElement) => HTMLElement)
  skip?: boolean
}

export interface TagParseRule extends GenericParseRule {
  tag: string
}

export interface StyleParseRule extends GenericParseRule {
  style: string
}

export type ParseRule = TagParseRule | StyleParseRule

export class DOMParser {
  readonly schema: any
  readonly rules: ParseRule[]
  constructor(schema: any, rules: ParseRule[])
  parse(dom: HTMLElement | DocumentFragment, options?: ParseOptions): any
  parseSlice(dom: HTMLElement | DocumentFragment, options?: ParseOptions): any
  static fromSchema(schema: any): DOMParser
  static schemaRules(schema: any): ParseRule[]
}
