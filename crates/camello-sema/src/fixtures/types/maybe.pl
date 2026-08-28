use strict;
use warnings;

package Row;
use Moose;
has id => (is => 'ro', isa => 'Int');

package main;

# Returns: Maybe[InstanceOf['Row']]
sub find { return undef }

# A `Maybe` used with nothing having checked it.
my $bare = find();
print $bare->id;                #~ info maybe-deref: may be undefined here

# Every one of these is a narrowing, and each is a fixture rather than a
# theorem (`docs/typecheck.md`, "Narrowing").
my $checked = find();
if (defined $checked) {
    print $checked->id;
}

my $truthy = find();
if ($truthy) {
    print $truthy->id;
}

my $guarded = find();
return unless defined $guarded;
print $guarded->id;

my $defaulted = find() // Row->new(id => 1);
print $defaulted->id;

# ----- what the shape of a condition says, and what it does not -----
#
# The list above is read structurally: a condition is a tree, and `!`, `||`
# and a call nobody read all change what its parts say. Read as a flat scan —
# "any variable named anywhere in the condition is defined below it" — every
# one of the four below was silent, and the first is a program that dies.

my $a = find();
if (!$a) {
    print $a->id;               #~ info maybe-deref: may be undefined here
}

my $b = find();
my $other = find();
if ($b || $other) {
    print $b->id;               #~ info maybe-deref: may be undefined here
}

my $c = find();
if (looks_ok($c)) {
    print $c->id;               #~ info maybe-deref: may be undefined here
}
sub looks_ok { return 1 }

my $d = find();
my $e = find();
if ($d && $e) {
    print $d->id, $e->id;
}

# A comparison has both its operands evaluated, so what either says holds —
# which is what `ref $x eq 'HASH'` is written for.
my $f = find();
if (ref $f eq 'Row') {
    print $f->id;
}

# The call in the condition is itself a `maybe-deref` — but it happened, so
# below it the invocant was there to be called.
my $g = find();
if ($g->id) {                   #~ info maybe-deref: may be undefined here
    print $g->id;
}

# `unless` runs its block where the condition did *not* hold, so what the
# condition says belongs to the `else`.
my $h = find();
unless (defined $h) {
    print 'nothing';
} else {
    print $h->id;
}

# A guard reads only the side that had to have held: the `die` is not part of
# the condition, and neither is anything else on the statement.
my $i = find();
$i or die 'no';
print $i->id;

# ----- short circuit, guards, and `elsif` -----

# perl runs the right side of `&&` only where the left held, so the call here
# is reached only when `$j` is there. Read left-to-right with no narrowing
# between, this was a `maybe-deref` on correct code — and it is how most of a
# real codebase writes the check.
my $j = find();
if (defined $j && $j->id) {
    print $j->id;
}

# The mirror of it: the right side of `||` runs only where the left failed.
my $k = find();
if (!$k || $k->id) {
    print 'either way';
}

# `LEAVE if COND` leaves the *opposite* of COND holding below it, which is
# what `return ... if !$x` is written for.
my $l = find();
return undef if !$l;
print $l->id;

my $m = find();
return undef if !$m || !$m->id;
print $m->id;

# A guard says nothing about a name it never tested. Read as a flat scan of
# the whole statement, the `$n` in the value being returned was narrowed by a
# guard that only ever looked at `$o`.
my $n = find();
my $o = find();
return $n // Row->new(id => 1) unless $o;
print $n->id;                   #~ info maybe-deref: may be undefined here

# An `elsif` carries a condition of its own, and it narrows its own block.
my $p = find();
if (0) {
    print 'no';
} elsif ($p) {
    print $p->id;
}

# The call in `!$x->name` ran before the `!` had anything to negate, so the
# invocant was there whichever way the condition then went.
my $q = find();
if (!$q->id) {                  #~ info maybe-deref: may be undefined here
    print $q->id;
}
