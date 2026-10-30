# EezFunnels public edition

Terms for the single-owner, self-hosted funnel product and its public visitor journey.

## Funnel content

**Funnel**:
A collection of pages organized under one funnel slug, with draft and published revisions.

**Page**:
A route-level part of a funnel with its own content and a position in the visitor journey.

**Revision**:
A complete immutable snapshot of a funnel's content and settings.

**Published revision**:
The revision currently served by the funnel's public routes.

**Section**:
A typed layout area on a page, such as a hero, features, testimonials, call to action, or footer.

**Block**:
A typed content or interaction item within a section.

**Form definition**:
A description of a form's purpose, fields, and completion result.

**Field**:
A named input with a type and validation bounds in a form definition.

**Asset**:
An immutable media item referenced by an image block.

**Theme**:
A set of design choices applied across a funnel.

## Visitor journey and owner work

**Submission**:
The field values and consent evidence accepted from one public form request. A submission records a request; it does not establish a booking, sale, or payment.

**Request acknowledgement**:
The required consent copy that explains the processing of a form request.

**Marketing consent**:
A separate, optional choice about marketing communication. It does not grant measurement consent.

**Measurement consent**:
A separate, optional choice that permits the collection of supported aggregate page and form interactions.

**Owner**:
The single operator responsible for the self-hosted EezFunnels installation. The public edition does not create application accounts or roles.

**Inbox**:
The owner's private view of retained submissions and their follow-up information.

**Qualification**:
An owner-recorded assessment that a submission meets the owner's stated criteria. It is not inferred from contact, reservation, sale, or payment.

**Reservation**:
An agreed appointment or place in a service. Form submission alone is not a reservation.

**Sale**:
A completed commercial transaction. A qualified or closed submission is not a sale or proof of payment.

**Payment**:
An amount successfully received and confirmed by a payment provider. EezFunnels does not process or confirm payments.

## Delivery and measurement

**Delivery destination**:
An HTTPS endpoint configured by the installation operator to receive selected submission data. Visitors cannot choose or change it.

**Delivery task**:
A durable attempt to hand an accepted submission to a configured destination. A task or its HTTP acknowledgement does not prove that a person acted on the request.

**Receiver acceptance**:
A successful HTTP response from the configured endpoint. It confirms only that the receiver accepted the request; it does not establish a unique downstream effect.

**Campaign**:
A configured label attached to an interaction on its current page after measurement consent. It does not identify a visitor or connect separate pages into a journey.

**Retained cohort**:
The submissions still present and eligible for a report during its stated time window.

_Avoid_: lead, conversion, customer, booking, or sale when the system only records a form submission or an aggregate interaction.
