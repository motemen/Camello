use strict;
use warnings;
use Base;
use Child;

# The chain the naive design reports on: `set_x` is written in `Base`, so a
# `Base` is what its `$self` is bound to — and `->extra` on the answer would
# be an `unknown-method` on every builder in a corpus.
Child->new->set_x(1)->extra;
Child->new->touched->extra;

# Substituted and still checked: `Child` really has no `nope`.
Child->new->set_x(1)->nope;     #~ warning unknown-method: `Child` declares no method `nope`

# A `Base` stays a `Base`.
Base->new->set_x(1)->extra;     #~ warning unknown-method: `Base` declares no method `extra`

# The `Maybe` survives the substitution, and it is about the receiver's class.
Child->new->if_ready->extra;    #~ warning maybe-deref: may be undefined here
