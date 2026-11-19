use strict;
use warnings;
use My::Vars;
use My::Globbed;

# What `@EXPORT` names, `*Glob` as all three, and what `import` hands over
# through a glob.
print $Default, @Glob, $Glob, $Config{name};

# `@EXPORT_OK` comes only when asked for.
print $Offered{x};    #~ error undeclared-variable: `%Offered`
