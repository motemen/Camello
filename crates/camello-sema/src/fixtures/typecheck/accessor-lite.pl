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
sub _build_lazily { 1 }
sub _build_once   { 2 }

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
print $mk->lazily, $mk->once;
print $mk->neither;             #~ warning unknown-method: `neither`
