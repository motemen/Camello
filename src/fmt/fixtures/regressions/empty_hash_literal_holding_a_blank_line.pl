# An empty hash literal with a blank line inside it. The first pass keeps the
# break and emits `{\n}`; the second pass, with no blank line left to preserve,
# collapses it to `{}` — so the fixed point is one pass away. Written without the
# blank line, as `baz` is, it reaches `{}` on the first pass.
package Foo;

__PACKAGE__->bar({

});

__PACKAGE__->baz({
});

use constant QUX => + {

};

1;
