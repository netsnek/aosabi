Aosabi generated an invalid GraphQL schema:



This usually means a resolver return type cannot form a valid schema on its own (e.g. an interface member missing a field, often from an ambiguous inferred type). Fixes: give the resolver an explicit return-type annotation; and for a polymorphic delegate, mark each patch branch's `__typename` `as const` so the variants are distinguishable.