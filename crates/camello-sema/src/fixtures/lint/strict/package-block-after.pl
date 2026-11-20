package Foo {
    use strict;
    use warnings;
    sub foo { 1 }
}
print "ok\n";  #~ warning missing-strict #~ warning missing-warnings
