# Ambiguous function calls - should preserve original formatting

# These are ambiguous: foo+1 could be foo(+1) or foo()+1
$x = foo+1;
$y = bar-2;
$z = baz+3;

# With spaces on one side - still ambiguous
$a = foo +1;
$b = bar -2;
$c = baz+ 3;

# With spaces on both sides - clearly binary operators
$d = foo + 1;
$e = bar - 2;

# With parentheses - not ambiguous
$f = foo(+1);
$g = bar(-2);
$h = baz(1);

# Mixed cases
$i = foo+1+2;
$j = foo+bar-baz;
$k = foo+ 1 +2;

# Built-in functions with ambiguous calls
print foo-2;
warn bar+1;
say baz-qux;
scalar foo+bar;

# Other prefix operators - also ambiguous
$l = foo!1;
$m = bar~2;
$n = baz\$x;
$o = qux++$y;
$p = quux--$z;
$q = corge not $w;

# With spaces - still ambiguous
$r = foo ! 1;
$s = bar ~ 2;
$t = baz \ $x;
