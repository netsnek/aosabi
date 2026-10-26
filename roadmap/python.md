# Python

The engine does not care which language a service is written in, and Python is the third language
Aosabi plans beside Rust and TypeScript. No line of the Python layer exists yet, and these are the
principles it starts from.

1. Type hints are the schema: a class with annotations, a dataclass or a `TypedDict` is an object
   type, `X | None` is nullable, `list[X]` is a list, an `Enum` is an enum, a `type` statement of a
   union is a union, a method is a field with arguments, a `@property` is a getter, and an
   `async def` resolves later.
2. Inheritance is native in Python, so a base class gives its fields to the classes that extend it
   as the schema expects.
3. Names follow the rule of the Rust layer, snake case printed in lower camel case.
4. Docstrings are the descriptions, and the description of a parameter comes from the parameter
   section of any of the three docstring styles Sphinx reads.
5. Exceptions, guards and the request are native: `raise` throws, a guard is a plain call first in
   the body, `contextvars` carries the request, and an async generator is a subscription.
6. The index is `app.py`, the one place that looks like a framework, and the layer adds no
   decorator, no base class and no marker of Aosabi to a type of the service.

How Python reaches the engine is still open. A shared library of the project with a C interface
loaded through `ctypes`, an extension over the stable interface of CPython, and the engine in a
process of its own that the Python app talks to over a pipe are the three ways the layer measures
before it takes one, under the [dependency rule](../CONTRIBUTING.md#5-dependencies).
