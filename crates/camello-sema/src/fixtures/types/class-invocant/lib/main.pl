use strict;
use warnings;
use Base;
use Child;

# `Self`: what `$class->new` built is of the class the caller named.
Child->build->extra;
Child->build_slowly->extra;
Child->built->extra;

# Substituted and still checked.
Child->build->nope;             #~ warning unknown-method: `Child` declares no method `nope`
Base->build->extra;             #~ warning unknown-method: `Base` declares no method `extra`

# `Base->new` written out is a `Base`, whoever called it.
Child->literal->extra;          #~ warning unknown-method: `Base` declares no method `extra`
Child->replaced->extra;         #~ warning unknown-method: `Base` declares no method `extra`

# A value of another class is that class, and the substitution leaves it alone.
Child->config->key;
Child->config->extra;           #~ warning unknown-method: `Config` declares no method `extra`
