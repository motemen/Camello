use strict;
use warnings;
use My::Named;

# A glob whose name is computed: nobody can say what the caller gets.
print $Settings{x};
