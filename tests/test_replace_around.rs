#![recursion_limit = "512"]
//! Regression test for a replaceAround step that was failing on a real-world
//! document using the "Standard Article" schema.
//!
//! The step changes the `hidden` attribute of the abstract `richtext_part`
//! from `true` to `false` by wrapping the existing content in a new node
//! with updated attributes.

use prosemirror::dynamic::types::Dyn;
use prosemirror::dynamic::DynamicSchema;
use prosemirror::transform::Step;

fn article_schema() -> DynamicSchema {
    DynamicSchema::from_json(&serde_json::json!({
        "nodes": {
            "doc": {"content":"title part*","selectable":false,"allowGapCursor":false,"attrs":{"documentstyle":{"default":""},"tracked":{"default":false},"citationstyle":{"default":"apa"},"citationstyles":{"default":["american-anthropological-association","apa","chicago-author-date","chicago-note-bibliography","harvard-cite-them-right","modern-language-association","nature","oxford-university-press-humsoc"]},"language":{"default":"en-US"},"languages":{"default":["af-ZA","sq-AL","ar","ast","be","br","bg","ca","ca-ES-Valencia","zh-CN","da","nl","en-AU","en-CA","en-NZ","en-ZA","en-GB","en-US","eo","fr","gl","de-DE","de-AU","de-CH","el","he","is","it","ja","km","lt","ml","nb-NO","nn-NO","fa","pl","pt-BR","pt-PT","ro","ru","tr","sr-SP-Cy","sr-SP-Lt","sk","sl","es","sv","ta","tl","uk"]},"papersize":{"default":"A4"},"papersizes":{"default":["A4","US Letter"]},"footnote_marks":{"default":["strong","em","link"]},"footnote_elements":{"default":["paragraph","heading1","heading2","heading3","heading4","heading5","heading6","figure","ordered_list","bullet_list","horizontal_rule","equation","citation","cross_reference","blockquote","table"]},"bibliography_header":{"default":{}},"template":{"default":""},"import_id":{"default":""},"copyright":{"default":{"holder":false,"year":false,"freeToRead":true,"licenses":[]}},"code_categories":{"default":{"listing":{"counter":0,"enabled":true},"example":{"counter":0,"enabled":true},"snippet":{"counter":0,"enabled":false},"tutorial":{"counter":0,"enabled":false},"exercise":{"counter":0,"enabled":false},"exercise_solution":{"counter":0,"enabled":false}}},"code_languages":{"default":["javascript","python","java","cpp","c","csharp","php","ruby","go","rust","swift","kotlin","typescript","html","css","sql","bash","shell","r","matlab","scala","perl","lua","haskell","xml","json","yaml","markdown"]},"id_types":{"default":[]}},"parseDOM":[{"tag":"div.doc"}]},
            "richtext_part": {"content":"block+","group":"part","marks":"annotation track","isolating":true,"attrs":{"title":{"default":""},"id":{"default":""},"locking":{"default":false},"language":{"default":false},"optional":{"default":false},"hidden":{"default":false},"help":{"default":false},"initial":{"default":false},"deleted":{"default":false},"elements":{"default":["paragraph","heading1","heading2","heading3","heading4","heading5","heading6","code_block","figure","ordered_list","bullet_list","horizontal_rule","equation","citation","cross_reference","blockquote","footnote","table"]},"marks":{"default":["strong","em","link"]},"metadata":{"default":false}},"parseDOM":[{"tag":"div.doc-richtext"}]},
            "heading_part": {"content":"heading","group":"part","marks":"annotation track","isolating":true,"attrs":{"title":{"default":""},"id":{"default":""},"locking":{"default":false},"language":{"default":false},"optional":{"default":false},"hidden":{"default":false},"help":{"default":false},"initial":{"default":false},"deleted":{"default":false},"elements":{"default":["heading1"]},"marks":{"default":["strong","em","link","anchor","sup","sub","code"]},"metadata":{"default":false}},"parseDOM":[{"tag":"div.doc-heading"}]},
            "contributors_part": {"content":"contributor*","group":"part","marks":"annotation track","isolating":true,"attrs":{"title":{"default":""},"id":{"default":""},"locking":{"default":false},"language":{"default":false},"optional":{"default":false},"hidden":{"default":false},"help":{"default":false},"initial":{"default":false},"deleted":{"default":false},"item_title":{"default":"Contributor"},"metadata":{"default":false}},"parseDOM":[{"tag":"div.doc-contributors"}]},
            "tags_part": {"content":"tag*","group":"part","marks":"annotation track","isolating":true,"attrs":{"title":{"default":""},"id":{"default":""},"locking":{"default":false},"language":{"default":false},"optional":{"default":false},"hidden":{"default":false},"help":{"default":false},"initial":{"default":false},"deleted":{"default":false},"item_title":{"default":"Tag"},"metadata":{"default":false}},"parseDOM":[{"tag":"div.doc-tags"}]},
            "table_part": {"content":"table","group":"part","marks":"annotation track","isolating":true,"attrs":{"title":{"default":""},"id":{"default":""},"locking":{"default":false},"language":{"default":false},"optional":{"default":false},"hidden":{"default":false},"help":{"default":false},"initial":{"default":false},"deleted":{"default":false},"elements":{"default":["paragraph","heading1","heading2","heading3","heading4","heading5","heading6","code_block","figure","ordered_list","bullet_list","horizontal_rule","equation","citation","blockquote","footnote"]},"marks":{"default":["strong","em","link","anchor","sup","sub","code"]}},"parseDOM":[{"tag":"div.doc-table"}]},
            "table_of_contents": {"group":"part","marks":"annotation track","defining":true,"parseDOM":[{"tag":"div.table-of-contents"}],"attrs":{"title":{"default":"Table of Contents"},"id":{"default":"toc"},"optional":{"default":false},"hidden":{"default":false}}},
            "separator_part": {"marks":"annotation track","group":"part","defining":true,"attrs":{"id":{"default":"separator"}},"parseDOM":[{"tag":"hr.doc-separator_part"}]},
            "title": {"content":"text*","marks":"annotation track","group":"fixedpart","defining":true,"isolating":true,"attrs":{"id":{"default":"title"}},"parseDOM":[{"tag":"div.doc-title"}]},
            "contributor": {"inline":true,"draggable":true,"attrs":{"firstname":{"default":false},"lastname":{"default":false},"email":{"default":false},"institution":{"default":false},"id_type":{"default":false},"id_value":{"default":false}},"parseDOM":[{"tag":"span.contributor"}]},
            "tag": {"inline":true,"draggable":true,"attrs":{"tag":{"default":""}},"parseDOM":[{"tag":"span.tag"}]},
            "paragraph": {"group":"block","content":"inline*","attrs":{"track":{"default":[]}},"parseDOM":[{"tag":"p"}]},
            "blockquote": {"content":"block+","group":"block","attrs":{"track":{"default":[]}},"marks":"annotation","defining":true,"parseDOM":[{"tag":"blockquote"}]},
            "horizontal_rule": {"group":"block","attrs":{"track":{"default":[]}},"parseDOM":[{"tag":"hr"}]},
            "figure": {"inline":false,"allowGapCursor":false,"selectable":true,"group":"block","attrs":{"category":{"default":"none"},"caption":{"default":false},"id":{"default":false},"track":{"default":[]},"aligned":{"default":"center"},"width":{"default":"100"}},"content":"figure_caption image|figure_caption figure_equation|image figure_caption|figure_equation figure_caption","parseDOM":[{"tag":"figure"}]},
            "image": {"selectable":false,"draggable":false,"attrs":{"image":{"default":false}},"parseDOM":[{"tag":"img"}]},
            "figure_equation": {"selectable":false,"draggable":false,"attrs":{"equation":{"default":false}},"parseDOM":[{"tag":"div.figure-equation[data-equation]"}]},
            "figure_caption": {"isolating":true,"defining":true,"content":"inline*","parseDOM":[{"tag":"figcaption span.text"}]},
            "heading1": {"group":"block heading","content":"inline*","marks":"_","defining":true,"attrs":{"id":{"default":false},"track":{"default":[]}},"parseDOM":[{"tag":"h1"}]},
            "heading2": {"group":"block heading","content":"inline*","marks":"_","defining":true,"attrs":{"id":{"default":false},"track":{"default":[]}},"parseDOM":[{"tag":"h2"}]},
            "heading3": {"group":"block heading","content":"inline*","marks":"_","defining":true,"attrs":{"id":{"default":false},"track":{"default":[]}},"parseDOM":[{"tag":"h3"}]},
            "heading4": {"group":"block heading","content":"inline*","marks":"_","defining":true,"attrs":{"id":{"default":false},"track":{"default":[]}},"parseDOM":[{"tag":"h4"}]},
            "heading5": {"group":"block heading","content":"inline*","marks":"_","defining":true,"attrs":{"id":{"default":false},"track":{"default":[]}},"parseDOM":[{"tag":"h5"}]},
            "heading6": {"group":"block heading","content":"inline*","marks":"_","defining":true,"attrs":{"id":{"default":false},"track":{"default":[]}},"parseDOM":[{"tag":"h6"}]},
            "code_block": {"content":"text*","marks":"_","group":"block","code":true,"defining":true,"attrs":{"track":{"default":[]},"language":{"default":""},"category":{"default":""},"title":{"default":""},"id":{"default":""}},"parseDOM":[{"tag":"pre","preserveWhitespace":"full"}]},
            "text": {"group":"inline"},
            "hard_break": {"inline":true,"group":"inline","selectable":false,"parseDOM":[{"tag":"br"}]},
            "citation": {"inline":true,"group":"inline","attrs":{"format":{"default":"autocite"},"references":{"default":[]}},"parseDOM":[{"tag":"span.citation"}]},
            "equation": {"inline":true,"group":"inline","attrs":{"equation":{"default":""}},"parseDOM":[{"tag":"span.equation"}]},
            "cross_reference": {"inline":true,"group":"inline","attrs":{"id":{"default":false},"title":{"default":null}},"parseDOM":[{"tag":"span.cross-reference[data-id][data-title]"}]},
            "footnote": {"inline":true,"group":"inline","attrs":{"footnote":{"default":[{"type":"paragraph"}]}},"parseDOM":[{"tag":"span.footnote-marker[data-footnote]"}]},
            "ordered_list": {"group":"block","content":"list_item+","attrs":{"id":{"default":false},"order":{"default":1},"track":{"default":[]}},"parseDOM":[{"tag":"ol"}]},
            "bullet_list": {"group":"block","content":"list_item+","attrs":{"id":{"default":false},"track":{"default":[]}},"parseDOM":[{"tag":"ul"}]},
            "list_item": {"content":"block+","marks":"annotation","attrs":{"track":{"default":[]}},"parseDOM":[{"tag":"li"}],"defining":true},
            "table": {"inline":false,"group":"block","tableRole":"table","attrs":{"id":{"default":false},"track":{"default":[]},"width":{"default":"100"},"aligned":{"default":"center"},"layout":{"default":"fixed"},"category":{"default":"none"},"caption":{"default":false}},"content":"table_caption table_body","parseDOM":[{"tag":"table"}]},
            "table_caption": {"content":"inline*","parseDOM":[{"tag":"caption span.text"}]},
            "table_body": {"content":"table_row+","tableRole":"table","isolating":true,"parseDOM":[{"tag":"tbody"}]},
            "table_row": {"content":"(table_cell | table_header)+","tableRole":"row","parseDOM":[{"tag":"tr"}]},
            "table_cell": {"marks":"annotation","content":"block+","attrs":{"colspan":{"default":1},"rowspan":{"default":1},"colwidth":{"default":null}},"tableRole":"cell","isolating":true,"parseDOM":[{"tag":"td"}]},
            "table_header": {"marks":"annotation","content":"block+","attrs":{"colspan":{"default":1},"rowspan":{"default":1},"colwidth":{"default":null}},"tableRole":"header_cell","isolating":true,"parseDOM":[{"tag":"th"}]}
        },
        "marks": {
            "em": {"parseDOM":[{"tag":"i"},{"tag":"em"},{"style":"font-style=italic"},{"style":"font-style=normal"}]},
            "strong": {"parseDOM":[{"tag":"strong"},{"tag":"b"},{"style":"font-weight=400"},{"style":"font-weight"}]},
            "link": {"attrs":{"href":{},"title":{"default":""}},"inclusive":false,"parseDOM":[{"tag":"a[href]"}]},
            "underline": {"parseDOM":[{"tag":"span.underline"}]},
            "sup": {"parseDOM":[{"tag":"sup"}],"excludes":"sub"},
            "sub": {"parseDOM":[{"tag":"sub"}],"excludes":"sup"},
            "code": {"parseDOM":[{"tag":"code"}],"excludes":"strong em underline link sup sub"},
            "comment": {"attrs":{"id":{"default":false}},"inclusive":false,"excludes":"","group":"annotation","parseDOM":[{"tag":"span.comment[data-id]"}]},
            "annotation_tag": {"attrs":{"type":{"default":""},"key":{"default":""},"value":{"default":""}},"inclusive":false,"excludes":"","group":"annotation","parseDOM":[{"tag":"span.annotation-tag[data-type]"}]},
            "anchor": {"attrs":{"id":{"default":false}},"inclusive":false,"group":"annotation","parseDOM":[{"tag":"span.anchor[data-id]"}]},
            "deletion": {"attrs":{"user":{"default":0},"username":{"default":""},"date":{"default":0}},"inclusive":false,"group":"track","parseDOM":[{"tag":"span.deletion"}]},
            "insertion": {"attrs":{"user":{"default":0},"username":{"default":""},"date":{"default":0},"approved":{"default":true}},"inclusive":false,"group":"track","parseDOM":[{"tag":"span.insertion"},{"tag":"span.approved-insertion"}]},
            "format_change": {"attrs":{"user":{"default":0},"username":{"default":""},"date":{"default":0},"before":{"default":[]},"after":{"default":[]}},"inclusive":false,"group":"track","parseDOM":[{"tag":"span.format-change"}]}
        }
    })).unwrap()
}

/// The document JSON from the failing example. The abstract `richtext_part`
/// (at positions 12..16) has `hidden: true` and contains a single empty
/// paragraph.
fn article_doc_json() -> serde_json::Value {
    serde_json::json!({
        "attrs":{"bibliography_header":{},"citationstyle":"apa","citationstyles":["american-anthropological-association","apa","chicago-author-date","chicago-note-bibliography","harvard-cite-them-right","modern-language-association","nature","oxford-university-press-humsoc"],"code_categories":{"example":{"counter":0,"enabled":true},"exercise":{"counter":0,"enabled":false},"exercise_solution":{"counter":0,"enabled":false},"listing":{"counter":0,"enabled":true},"snippet":{"counter":0,"enabled":false},"tutorial":{"counter":0,"enabled":false}},"code_languages":["javascript","python","java","cpp","c","csharp","php","ruby","go","rust","swift","kotlin","typescript","html","css","sql","bash","shell","r","matlab","scala","perl","lua","haskell","xml","json","yaml","markdown"],"copyright":{"freeToRead":true,"holder":false,"licenses":[],"year":false},"documentstyle":"elephant","footnote_elements":["paragraph","heading1","heading2","heading3","heading4","heading5","heading6","figure","ordered_list","bullet_list","horizontal_rule","equation","citation","cross_reference","blockquote","table"],"footnote_marks":["strong","em","link"],"id_types":[],"import_id":"standard-article","language":"en-US","languages":["af-ZA","sq-AL","ar","ast","be","br","bg","ca","ca-ES-Valencia","zh-CN","da","nl","en-AU","en-CA","en-NZ","en-ZA","en-GB","en-US","eo","fr","gl","de-DE","de-AU","de-CH","el","he","is","it","ja","km","lt","ml","nb-NO","nn-NO","fa","pl","pt-BR","pt-PT","ro","ru","tr","sr-SP-Cy","sr-SP-Lt","sk","sl","es","sv","ta","tl","uk"],"papersize":"A4","papersizes":["A4","US Letter"],"template":"Standard Article","tracked":false},
        "content":[
            {
                "content":[{"marks":[{"attrs":{"approved":true,"date":29655180,"user":1,"username":"Yeti"},"type":"insertion"}],"text":"Test","type":"text"}],
                "type":"title"
            },
            {
                "attrs":{"hidden":true,"id":"subtitle","initial":[{"attrs":{"id":"H6323428"},"type":"heading1"}],"marks":["strong","em","link"],"metadata":"subtitle","optional":"hidden","title":"Subtitle"},
                "content":[{"attrs":{"id":"H6323428"},"type":"heading1"}],
                "type":"heading_part"
            },
            {
                "attrs":{"hidden":true,"id":"authors","item_title":"Author","metadata":"authors","optional":"hidden","title":"Authors"},
                "type":"contributors_part"
            },
            {
                "attrs":{"hidden":true,"id":"abstract","marks":["strong","em","link"],"metadata":"abstract","optional":"hidden","title":"Abstract"},
                "content":[{"type":"paragraph"}],
                "type":"richtext_part"
            },
            {
                "attrs":{"hidden":true,"id":"keywords","item_title":"Keyword","metadata":"keywords","optional":"hidden","title":"Keywords"},
                "type":"tags_part"
            },
            {
                "attrs":{"id":"body","marks":["strong","em","link"],"title":"Body"},
                "content":[{"type":"paragraph"}],
                "type":"richtext_part"
            }
        ],
        "type":"doc"
    })
}

/// The replaceAround step that changes `hidden: true` -> `hidden: false` on
/// the abstract part by wrapping the existing paragraph in a new richtext_part
/// with updated attributes.
///
/// Positions in this document:
/// - title:              [0..6]   (node_size = 2 + 4 chars)
/// - heading_part:       [6..10]  (node_size = 2 + 2)
/// - contributors_part:  [10..12] (node_size = 2, no content)
/// - richtext_part (abstract): [12..16] (node_size = 2 + 2)
///   - paragraph:        [13..15] (node_size = 2, empty)
/// - tags_part:          [16..18] (node_size = 2)
/// - richtext_part (body): [18..22] (node_size = 2 + 2)
fn replace_around_step_json() -> serde_json::Value {
    serde_json::json!({
        "stepType": "replaceAround",
        "from": 12,
        "to": 16,
        "gapFrom": 13,
        "gapTo": 15,
        "insert": 1,
        "slice": {
            "content": [{
                "type": "richtext_part",
                "attrs": {
                    "title": "Abstract",
                    "id": "abstract",
                    "locking": false,
                    "language": false,
                    "optional": "hidden",
                    "hidden": false,
                    "help": false,
                    "initial": false,
                    "deleted": false,
                    "elements": ["paragraph","heading1","heading2","heading3","heading4","heading5","heading6","code_block","figure","ordered_list","bullet_list","horizontal_rule","equation","citation","cross_reference","blockquote","footnote","table"],
                    "marks": ["strong","em","link"],
                    "metadata": "abstract"
                }
            }]
        },
        "structure": true
    })
}

#[test]
fn replace_around_unhides_abstract_part() {
    let schema = article_schema();

    schema.with_types(|| {
        let doc = schema
            .node_from_json(&article_doc_json())
            .expect("document should parse");

        // Verify our assumed position layout before applying the step.
        // The abstract richtext_part is the 4th child (index 3) of the doc.
        use prosemirror::model::Node;
        let abstract_part = doc.child(3).expect("doc should have 4th child");
        assert_eq!(abstract_part.type_name, "richtext_part");
        let abstract_attrs = abstract_part.attrs_json();
        assert_eq!(
            abstract_attrs["hidden"], true,
            "abstract part should start hidden"
        );

        let step: Step<Dyn> =
            serde_json::from_value(replace_around_step_json()).expect("step should deserialize");

        let result = step.apply(&doc);

        let new_doc = result.expect("replaceAround step should succeed");

        // The abstract part should now have hidden: false.
        let new_abstract = new_doc.child(3).expect("doc should still have 4th child");
        assert_eq!(new_abstract.type_name, "richtext_part");
        let new_attrs = new_abstract.attrs_json();
        assert_eq!(
            new_attrs["hidden"], false,
            "abstract part should be unhidden after the step"
        );

        // The paragraph inside should be preserved.
        assert_eq!(new_abstract.child_count(), 1);
        let inner = new_abstract
            .child(0)
            .expect("abstract should still have a paragraph");
        assert_eq!(inner.type_name, "paragraph");
    });
}
