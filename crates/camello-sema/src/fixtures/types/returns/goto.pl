use strict;
use warnings;

# `goto &NAME` is a tail call in the strict sense: perl replaces the frame,
# `@_` and all, so what the caller receives is the target's answer
# (`docs/return-inference.md`, "A `goto` is a tail call"). `Path::Tiny::path`
# ends this way, and while every `goto` was opaque the whole chain below it
# was `Unknown`.

package Slot;
use Moose;
has n => (is => 'ro', isa => 'Int');

# The target's answer, straight through.
package Direct;

sub make { goto &_build }

sub _build { bless {}, 'Direct' }

# Two hops, and an `unshift @_` in between — which is what a `goto` is written
# for and what the return has nothing to do with.
package Chained;

sub make {
    shift;
    _make(@_)
}

sub _make {
    unshift @_, 1;
    goto &_pathify
}

sub _pathify { bless {}, 'Chained' }

# A qualified target names the sub outright, and the class is whatever *it*
# blesses into — not the package the `goto` was written in.
package Qualified;

sub make { goto &Direct::_build }

# ----- and what stays opaque, which is silent -----

# A coderef names a sub nobody here read.
package Coderef;

sub make {
    my $code = \&Direct::_build;
    goto &$code;
}

# A plain scalar is the same.
package Scalarish;

sub make {
    my $code = shift;
    goto $code;
}

package main;

print Slot->new(n => Direct::make())->n;       #~ warning type-mismatch: `Direct::make()`
print Slot->new(n => Chained::make())->n;      #~ warning type-mismatch: `Chained::make()`
print Slot->new(n => Qualified::make())->n;    #~ warning type-mismatch: InstanceOf['Direct']
print Slot->new(n => Coderef::make())->n;
print Slot->new(n => Scalarish::make())->n;
