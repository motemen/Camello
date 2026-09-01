# A stub is ordinary Perl and goes through the ordinary declaration pass.
# This is the `.pyi` idea with no new syntax: a project types the corner of a
# dependency that no recogniser can read — XS, here — and the stub shadows the
# real module's declarations wholesale.
package DBI::db;

# Returns: Maybe[DBI::st]
sub prepare ($self, $sql) {}

# Returns: Maybe[HashRef]
sub selectrow_hashref ($self, $sql, $attr = undef, @bind) {}

1;
