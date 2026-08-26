# A package block written with a trailing `;`. The semicolon belongs to the
# `}` that precedes it, but it is put on a line of its own.
use Test2::V0;

sub t {
    package Foo::Bar {
        use parent -norequire, 'Foo::Base';
    };

    is 1, 1;
}

# The same shape after a definition, which asks for a blank line after itself:
# the definition ends at the semicolon, so the blank line goes after that.
sub u { 1 };

# And a `;` the writer did put on a line of its own stays there.
package Baz {
    1;
}
;
