# An anchor with nothing after it on the line has nothing to align: the operator
# it was going to hold went to the next line. Padding it left spaces at the end
# of a line, which the next pass trims — so the output was not its own fixed
# point. HTTP::Status writes this.
sub is_cacheable_by_default {
    $_[0] && (
        $_[0] == 200    # OK
            || $_[0] == 203    # Non-Authoritative Information
            || $_[0] == 204    # No Content
            || $_[0] == 300    # Multiple Choices
    );
}
