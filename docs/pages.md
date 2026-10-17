# Pages and steps

The typed editor supports up to 50 pages per funnel. Select **Add page** to create a step from the current page. The editor adds a **Continue** button that routes visitors to the new page. Choose a page from **Current page** to edit its sections and content.

Each non-entry page has an editable title and URL slug. Page slugs use lowercase letters, numbers, and hyphens, and must be unique within the funnel. The entry page keeps the funnel's root route. Published entry pages use `/f/:slug`; additional pages use `/f/:slug/:page_slug`.

Deleting a page asks for confirmation. It removes buttons that navigate to that page, changes forms that redirect there to show the inline message “Thank you.”, and removes forms placed on the deleted page. The edit is rejected if it would make another page unreachable from the entry page. Undo the deletion before saving to restore the previous draft.

Page edits remain drafts until a new immutable revision is saved. Publish the saved revision to make the changes available to visitors.
