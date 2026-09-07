import type { LayoutContactsProps } from "@generated/client";

/** Everyone on any wave. `here` is presence across every wave at once, which the field knows because a connection is an agent on it. */
export default function Contacts({ people }: LayoutContactsProps) {
  return (
    <ul className="contacts">
      {people.map((person) => (
        <li key={person.name} className={person.here ? "contact on" : "contact"}>
          <span className="dot" />
          <span className="contact-name">{person.name}</span>
          <span className="contact-waves">{person.waves}</span>
        </li>
      ))}
    </ul>
  );
}
