use strict;
use warnings;

package Store;

sub fetch {
    my ($self, $sql) = @_;
    # The stub's signature is what these are checked against, and no
    # diagnostic is ever reported against the stub itself.
    my $statement = DBI::db->prepare($sql);
    my $row = DBI::db->selectrow_hashref($sql);
    my $wrong = DBI::db->prepare;
    #~ error arity: takes at least 2 arguments including its invocant; 1 passed
    return ($statement, $row, $wrong);
}

1;
