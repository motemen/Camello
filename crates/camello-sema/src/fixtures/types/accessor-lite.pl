# The shapes are `Class-Accessor-Lite-0.08/t/00-base.t` and `t/02-use.t`, and
# `Class-Accessor-Lite-Lazy-0.03/t/01_use.t` and `t/02_mk_accessors.t`, which is
# where a library's real calling conventions are written down.
use strict;
use warnings;

package L;
use Class::Accessor::Lite (
    new => 1,
    rw  => [ qw(foo bar baz) ],
    ro  => [ qw(tokuhirom) ],
    wo  => [ qw(yappo) ],
);

package K;
use Class::Accessor::Lite;
# Installed into the caller, so these are `K`'s however the module is named.
Class::Accessor::Lite->mk_accessors(qw(foo bar));
Class::Accessor::Lite->mk_ro_accessors(qw(ro));
Class::Accessor::Lite->mk_wo_accessors(qw(wo));

package N;
use Class::Accessor::Lite;
Class::Accessor::Lite->mk_new_and_accessors(qw(alpha beta));

package Lazy;
use Class::Accessor::Lite::Lazy (
    new     => 1,
    ro      => ['foo'],
    rw      => ['bar'],
    ro_lazy => [ 'hoge', { poyo => \&make_poyo, poe => 'make_poe' } ],
    rw_lazy => [ 'fuga', 'attr_without_builder', { baz => 'make_baz' } ],
);
sub _build_hoge { 'xxx' }
sub _build_fuga { 'yyy' }
sub make_poyo   { 'poyo' }
sub make_poe    { 'poe' }
sub make_baz    { 1 }

package Mk;
use Class::Accessor::Lite::Lazy;
Class::Accessor::Lite::Lazy->mk_new;
Class::Accessor::Lite::Lazy->mk_lazy_accessors('lazily');
Class::Accessor::Lite::Lazy->mk_ro_lazy_accessors('once');
# The lazy makers flatten a hashref into name-and-builder pairs, the same as
# the `use` statement's does, so its keys name accessors too.
Class::Accessor::Lite::Lazy->mk_lazy_accessors('spelled', { built => 'make_built' });
Class::Accessor::Lite::Lazy->mk_ro_lazy_accessors({ anon => sub { 4 } });
sub _build_lazily  { 1 }
sub _build_once    { 2 }
sub _build_spelled { 3 }
sub make_built     { 4 }

# The family carries no types — except a lazy slot, which carries its builder,
# and what the builder returns is what the accessor hands back (ANNOT-10d).
package Built;
use Class::Accessor::Lite::Lazy (
    new     => 1,
    ro      => ['plain'],
    ro_lazy => [ 'implicit', { named => 'make_named', reffed => \&make_reffed } ],
    rw_lazy => { viahash => '_build_viahash', anon => sub { L->new } },
);
sub _build_implicit { L->new }
sub make_named      { L->new }
sub make_reffed     { L->new }
sub _build_viahash  { L->new }

package Inherited;
our @ISA = ('Built');
# The builder is reached as a method, so the subclass's own answers.
sub _build_implicit { N->new }

package main;

my $l = L->new(bar => 1, tokuhirom => 2);
$l->bar(3);
print $l->foo, $l->baz, $l->tokuhirom;
$l->yappo(4);
print $l->nope;                 #~ warning unknown-method: `nope`

# The generated `new` blesses the hash it is handed, so a key with no accessor
# behind it is still readable as `$self->{undeclared}` and the program may well
# be right. Worth saying, one severity below what it would be against a
# constructor that rejects the key.
my $spare = L->new(bar => 1, undeclared => 2);
#~ warning unknown-key: `undeclared`
print $spare->bar;

# Nothing here is ever required: the constructor never looks at what it was
# given, so there is nothing for it to find missing.
L->new;

# `K` never asked for a constructor, and nothing else gives it one — so the
# object comes from a `bless`, as it does in the module's own test.
K->new;                         #~ warning unknown-method: `new`
my $k = bless { foo => 1, ro => 3, wo => 4 }, 'K';
print $k->foo, $k->bar, $k->ro;
$k->wo(1);
print $k->missing;              #~ warning unknown-method: `missing`

my $n = N->new(alpha => 1);
print $n->alpha, $n->beta;
print $n->gamma;                #~ warning unknown-method: `gamma`

my $lazy = Lazy->new(foo => 1);
print $lazy->foo, $lazy->bar, $lazy->hoge, $lazy->poyo, $lazy->poe;
print $lazy->fuga, $lazy->attr_without_builder, $lazy->baz;
print $lazy->absent;            #~ warning unknown-method: `absent`

my $mk = Mk->new;
print $mk->lazily, $mk->once, $mk->spelled, $mk->built, $mk->anon;
print $mk->neither;             #~ warning unknown-method: `neither`

# A lazy slot is typed by its builder, however the builder was named.
my $built = Built->new;
$built->implicit->tokuhirom;
$built->named->tokuhirom;
$built->reffed->tokuhirom;
$built->viahash->tokuhirom;
$built->implicit->absent;       #~ warning unknown-method: `absent`
$built->named->absent;          #~ warning unknown-method: `absent`
$built->reffed->absent;         #~ warning unknown-method: `absent`
$built->viahash->absent;        #~ warning unknown-method: `absent`
# An anonymous builder names no sub to ask, and a slot that is not lazy has no
# builder at all: both stay `Unknown`, and nothing is reported against one.
$built->anon->absent;
$built->plain->absent;
# The subclass's `_build_implicit` is the one the accessor reaches.
Inherited->new->implicit->alpha;
Inherited->new->implicit->absent;   #~ warning unknown-method: `absent`
