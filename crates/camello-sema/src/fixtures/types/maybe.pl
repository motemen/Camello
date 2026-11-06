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
print $bare->id;                #~ warning maybe-deref: may be undefined here

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

my $exited = find();
exit unless defined $exited;
print $exited->id;

my $thrown = find();
My::Error->throw unless defined $thrown;
print $thrown->id;

my $done = find();
defined $done or do { warn 'none'; return };
print $done->id;

# A `do` block whose last statement may fall through does not leave.
my $maybe_done = find();
defined $maybe_done or do { return if $ENV{QUIET} };
print $maybe_done->id;          #~ warning maybe-deref: may be undefined here

my $defaulted = find() // Row->new(id => 1);
print $defaulted->id;

# A guard inside an anonymous sub or an `eval` leaves that, not the statement
# that holds it, so nothing below the statement has been checked.
my $called = find();
my $callback = sub { return unless defined $called; 1 };
print $called->id;              #~ warning maybe-deref: may be undefined here
$callback->();

my $tried = find();
my $ok = eval { defined $tried or die 'no'; 1 };
print $tried->id;               #~ warning maybe-deref: may be undefined here
print $ok;

# An attribute's argument list is not a list of targets: this is one scalar,
# assigned in scalar context.
my $attributed :Checked(1) = find();
print $attributed->id;          #~ warning maybe-deref: may be undefined here

# ----- what the shape of a condition says, and what it does not -----
#
# The list above is read structurally: a condition is a tree, and `!`, `||`
# and a call nobody read all change what its parts say. Read as a flat scan —
# "any variable named anywhere in the condition is defined below it" — every
# one of the four below was silent, and the first is a program that dies.

my $a = find();
if (!$a) {
    print $a->id;               #~ warning maybe-deref: may be undefined here
}

my $b = find();
my $other = find();
if ($b || $other) {
    print $b->id;               #~ warning maybe-deref: may be undefined here
}

my $c = find();
if (looks_ok($c)) {
    print $c->id;               #~ warning maybe-deref: may be undefined here
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
if ($g->id) {                   #~ warning maybe-deref: may be undefined here
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
print $n->id;                   #~ warning maybe-deref: may be undefined here

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
if (!$q->id) {                  #~ warning maybe-deref: may be undefined here
    print $q->id;
}

# ----- a modifier, a ternary and a loop say what an `if` says -----

# perl tests a modifier before it runs the statement, so the statement is read
# under the condition — and only the statement: below it the check is gone.
my $r = find();
$r->id if $r;
$r->id if defined $r;
$r->id unless !$r;
$r->id;                         #~ warning maybe-deref: may be undefined here

my $s = find();
$s->id unless $s;               #~ warning maybe-deref: may be undefined here

# Below a modified statement the two ways it went are joined, so a default
# assigned where the value was missing leaves it there either way.
my $t = find();
$t = Row->new(id => 1) unless $t;
print $t->id;

# `do { ... } while COND` runs its block once before the first test.
my $u = find();
do { print $u->id } while $u;   #~ warning maybe-deref: may be undefined here

my $v = find();
my $from_v = $v ? $v->id : 0;
my $from_v_too = !defined $v ? 0 : $v->id;
my $wrong_side = $v ? 0 : $v->id; #~ warning maybe-deref: may be undefined here
print $from_v, $from_v_too, $wrong_side;

# A loop's condition is tested again before every pass of its body.
my $w = find();
while ($w) {
    print $w->id;
    $w = find();
}
until (!$w) {
    print $w->id;
    $w = find();
}
while (!$w) {
    print $w->id;               #~ warning maybe-deref: may be undefined here
}
print $w->id;                   #~ warning maybe-deref: may be undefined here

# ----- a branch that leaves takes nothing below the `if` with it -----

# Below an `if` with no `else` is the way past its condition, where the
# condition did not hold — and the branch that returned is not part of it.
my $ret = find();
if (!$ret) { return; }
print $ret->id;

my $ret_unless = find();
unless (defined $ret_unless) {
    warn "missing";
    return;
}
print $ret_unless->id;

my $ret_wrong = find();
if ($ret_wrong) { return; }
print $ret_wrong->id;           #~ warning maybe-deref: may be undefined here

my $ret_elsif = find();
if (!$ret_elsif) {
    return;
} elsif ($ret_elsif->id) {
    print "positive";
}
print $ret_elsif->id;

# A branch that may come back still joins.
my $ret_maybe = find();
if (!$ret_maybe) {
    return if rand;
}
print $ret_maybe->id;           #~ warning maybe-deref: may be undefined here

# A default assigned where the value was missing leaves it there either way.
my $filled_in = find();
if (!$filled_in) {
    $filled_in = Row->new(id => 1);
}
print $filled_in->id;

# ----- a sub that ends in `die` leaves as `die` does -----

sub fail { die "failed" }
sub fail_maybe { die "failed" if rand }
sub fail_return { return }

my $failed = find();
fail() unless $failed;
print $failed->id;

my $failed_or = find();
$failed_or or fail();
print $failed_or->id;

my $failed_maybe = find();
fail_maybe() unless $failed_maybe;
print $failed_maybe->id;        #~ warning maybe-deref: may be undefined here

my $failed_return = find();
fail_return() unless $failed_return;
print $failed_return->id;       #~ warning maybe-deref: may be undefined here
