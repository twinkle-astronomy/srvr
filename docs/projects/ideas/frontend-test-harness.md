# Frontend Test Harness

The dashboard's UI components currently have no automated tests — they are only
verified by the fact that they compile. There is no way to assert that a
component actually renders the right thing or behaves correctly, so visual and
interaction regressions in the dashboard can land without anything catching them.

This would let us write tests that render a component and make assertions about
its output and behaviour — for example, that an empty list shows its
empty-state, that a populated result renders the expected summary rather than raw
data, or that a control appears only when it should. The goal is to cover the UI
the way the server-side logic is already covered, so changes to pages and
components can be made with the same confidence as changes to the backend.

Removes the current blind spot where the only signal that a frontend change is
correct is "it compiled and looked right when someone clicked through it."
