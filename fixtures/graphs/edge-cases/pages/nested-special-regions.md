- Nested special regions
  #+BEGIN_QUOTE
  intro
  #+BEGIN_NOTE
  - item one
    - item two
  ## Heading in a note
  ```rust
  let a = 1;
  ```
  #+END_NOTE
  #+END_QUOTE
- #+BEGIN_FOO bar
  - inside an unknown region
  #+END_FOO
- #+BEGIN_EXPORT html
  <p>raw</p>
  #+END_EXPORT
