Consult docs/architecture.md

CHANGELOG.md is for the people who read it. One line per change — two or three
where it takes them, never a paragraph. The reasoning, the measurements and the
caveats go in the commit the change came from, in a comment, or in `docs/`; the
entry cites the rule id and stops there. The released 0.1.1 section is the
grain to match.

`docs/` is the specification, and a specification is in the present tense. It
says what camello does and why that is the rule. It never says what camello
used to do, what a change fixed, or which half was missing before — the commit
log holds that, and holding it twice is how the two drift apart. A rule a
reader would arrive at unaided is one not to write down at all.
