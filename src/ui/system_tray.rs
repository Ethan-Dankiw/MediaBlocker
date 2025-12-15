use tray_icon::menu::{CheckMenuItem, IsMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem};

pub struct SystemTrayBuilder {
    // The items for the system tray menu
    items: Vec<Box<dyn IsMenuItem>>,
}

impl SystemTrayBuilder {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
        }
    }

    pub fn create_menu_item(&mut self, title: &str) -> MenuId {
        // Create the menu item
        let item = MenuItem::new(title, true, None);
        // Add the item to the menu
        self.add_item(item)
    }

    pub fn create_check_menu_item(&mut self, title: &str, default: bool) -> MenuId {
        // Create the checkbox menu item
        let item = CheckMenuItem::new(title, true, default, None);
        // Add the item to the menu
        self.add_item(item)
    }

    pub fn create_separator(&mut self) -> MenuId {
        // Create the separator menu item
        let item = PredefinedMenuItem::separator();
        // Add the item to the menu
        self.add_item(item)
    }

    pub fn build(&self) -> Menu {
        // Create a new menu
        let menu = Menu::new();

        // Add the items to the menu
        for item in &self.items {
            let _ = menu.append(item.as_ref());
        }

        // Return the menu
        menu
    }

    fn add_item<T>(&mut self, item: T) -> MenuId where T: IsMenuItem + 'static {
        // Create the box of the item
        let boxed: Box<dyn IsMenuItem> = Box::new(item);

        // Get the ID of the item
        let id = boxed.id().clone();

        // Add the item to the menu
        self.items.push(boxed);

        // Return the item ID
        id
    }
}




