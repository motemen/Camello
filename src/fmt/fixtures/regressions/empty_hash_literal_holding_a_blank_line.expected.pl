# An empty hash literal with a blank line inside it. A blank line separates one
# thing from another, and there is nothing here on either side of it, so it
# closes up the same way `baz` — written without one — already does.
package Foo;

__PACKAGE__->bar({});

__PACKAGE__->baz({});

use constant QUX => +{};

1;
