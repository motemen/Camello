use strict;
use warnings;

# What a single file can read off its own bodies, with no `Returns:` anywhere
# (`docs/return-inference.md`, "Tier 1"). Every diagnostic below is one the
# checker had nothing to say about before: the type it is about was `Unknown`.

package Row;
use Moose;
has id => (is => 'ro', isa => 'Int');

package Store;

# A constructor call, so the sub is an instance of the class it built.
sub row { return Row->new(id => 1) }

# A `bless` says the same thing without a framework behind it.
sub blessed { return bless {}, 'Row' }

# A literal.
sub count { return 42 }

# The tail of a body, which is where half a corpus keeps its constants.
sub limit { 10 }

# A reference literal.
sub listed { return [] }

# A hash written with literal keys is a `Dict`, and a slurpy one.
sub config { return { size => 1 } }

# `return;` is `Undef`, so a sub that has one of those among its object
# returns is a `Maybe` — the most useful diagnostic and the most likely false
# positive, at the severity the design already gives it.
sub find {
    my ($self, $id) = @_;
    return unless $id;
    return Row->new(id => $id);
}

# A `die` is not a site: it contributes nothing to the join, so this one is a
# `Row` and not a `Maybe[Row]`.
sub demand {
    my ($self, $id) = @_;
    die 'no id' unless $id;
    return Row->new(id => $id);
}

# An `if` chain with an `else` joins its branches' tails.
sub either {
    my ($self, $ok) = @_;
    if ($ok) {
        return Row->new(id => 1);
    }
    else {
        return Row->new(id => 2);
    }
}

# `wantarray ? LIST : SCALAR` is the scalar branch, by definition of what it
# asked.
sub both {
    my ($self) = @_;
    return wantarray ? (1, 2) : Row->new(id => 3);
}

# ----- and what stays `Unknown`, which is silent -----

# A list, whose scalar reading is a count or its last element and neither is a
# type the program has.
sub pair { return (1, 2) }

# An empty body.
sub todo { }

# A loop as the tail.
sub looping {
    my ($self) = @_;
    for my $x (1 .. 3) {
        $x++;
    }
}

# An `if` chain with no `else`, whose false value is its condition's.
sub perhaps {
    my ($self, $ok) = @_;
    if ($ok) {
        Row->new(id => 1);
    }
}

# A `goto` hands the call over to a sub this walk never looked at.
sub delegated { goto &row }

package main;

Store->row->id;
Store->row->nope;               #~ warning unknown-method: `Row` declares no method `nope`
Store->blessed->nope;           #~ warning unknown-method: `Row` declares no method `nope`
Store->either->nope;            #~ warning unknown-method: `Row` declares no method `nope`
Store->both->nope;              #~ warning unknown-method: `Row` declares no method `nope`
Store->demand(1)->nope;         #~ warning unknown-method: `Row` declares no method `nope`

# The `Maybe` the `return;` put there.
Store->find(1)->id;             #~ warning maybe-deref: may be undefined here

# What the inferred value is, against a slot that was declared.
Row->new(id => Store->count);
Row->new(id => Store->limit);
Row->new(id => Store->listed);  #~ warning type-mismatch
Row->new(id => Store->config);  #~ warning type-mismatch

# A `Dict` read off a body knows its keys, and says so about the other ones.
Store->config->{size};
Store->config->{nope};

# Nothing follows from what stayed `Unknown`.
Store->pair->nope;
Store->todo->nope;
Store->looping->nope;
Store->perhaps->nope;
Store->delegated->nope;
