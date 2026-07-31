# The core class feature: `class NAME;` opens a package-like scope, `field`
# declares a slot with a list of attributes and an optional default, and
# `method` is a `sub` with an implicit `$self`. `field` and `method` are
# keywords only inside a `class`.
use v5.38;
use experimental 'class';

class Foo;

field $bar : param;
field $baz : param;
field $qux : param : reader = undef;

method one () {
    return $bar->quux;
}

method two () { $qux // 'none' }

method three ($corge = undef) {
    return defined $corge ? 'yes' : 'no';
}
