# The shapes are `Class-Accessor-Typed-0.03/t/01_base.t`, `t/02_does.t` and
# `t/03_lazy.t`. This is the one of the accessor families that carries types,
# so it is the one where the constructor's arguments can be checked.
use strict;
use warnings;

package L;
use Class::Accessor::Typed (
    rw => {
        rw1 => { isa => 'Str', default => 'default value' },
        rw2 => 'Int',
    },
    ro => {
        ro1 => 'Str',
        ro2 => 'Int',
    },
    wo => {
        wo => 'Int',
    },
);

package M;
# `new => 0` is how a class keeps the accessors and writes its own constructor.
use Class::Accessor::Typed (
    rw  => { rw => 'Str' },
    new => 0,
);

package N;
use Class::Accessor::Typed (
    rw => {
        rw1 => 'Str',
        rw2 => { isa => 'Int', optional => 1 },
    },
);

package Lazy;
use Class::Accessor::Typed (
    rw_lazy => {
        rw1 => { isa => 'Str', default => 'default' },
        rw2 => { isa => 'Str', builder => 'builder_for_rw2' },
        rw3 => 'Int',
    },
    ro_lazy => {
        ro1 => { isa => 'Str', default => 'default' },
        ro2 => { isa => 'Str', builder => 'builder_for_ro2' },
    },
);
sub _build_rw1       {'rw1'}
sub _build_rw3       {1}
sub builder_for_rw2  {'rw2'}
sub _build_ro1       {'ro1'}
sub builder_for_ro2  {'ro2'}

package Role;
use Mouse::Role;

package Doer;
use Mouse;
with 'Role';

package Other;
use Mouse;

package Held;
use Class::Accessor::Typed (rw => { held => 'Role' });

package main;

my $l = L->new(rw1 => 'RW1', rw2 => 321, ro1 => 'RO1', ro2 => 123, wo => 222);
print $l->rw1, $l->rw2, $l->ro1, $l->ro2;
$l->rw1('x');
$l->wo(1);
print $l->nope;                 #~ warning unknown-method: `nope`

# The module dies on this one; both sides are written down, so it is an error.
L->new(rw1 => 'RW1', rw2 => 'RW2', ro1 => 'RO1', ro2 => 123, wo => 222);
#~ error type-mismatch: `rw2`

# A key with no attribute behind it: the value is dropped and the module warns.
L->new(rw1 => 'RW1', rw2 => 1, ro1 => 'x', ro2 => 2, wo => 3, unknown => 'unknown');
#~ error unknown-key: `unknown`

# Every slot is mandatory here unless it says `optional`, gives a `default`, or
# is lazy — the reverse of Moose's rule, and what the generated `new` dies on.
L->new(rw1 => 'RW1');           #~ error missing-argument: requires `rw2`

# `new => 0` means what it says.
M->new(rw => 'RW');             #~ warning unknown-method: `new`

# `optional => 1` is not a diagnostic either way.
my $n = N->new(rw1 => 'RW1');
print $n->rw1, $n->rw2;

# Every lazy slot has a builder, so `new` may be given nothing at all.
my $lazy = Lazy->new;
print $lazy->rw1, $lazy->rw2, $lazy->rw3, $lazy->ro1, $lazy->ro2;

# A role name in a type position is satisfied by a class that consumes it.
Held->new(held => Doer->new);
Held->new(held => Other->new);  #~ warning type-mismatch: `held`
