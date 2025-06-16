# API Key Management Frontend

This directory contains the frontend implementation for API key management in the Oxy web application.

## Components

### Pages
- **`/src/pages/api-keys/index.tsx`** - Main API key management page with full CRUD functionality

### Sidebar Navigation
- **`/src/components/AppSidebar/ApiKeys.tsx`** - Sidebar navigation component for API keys

### Services
- **`/src/services/apiKeyService.ts`** - Service layer for API key operations
- **`/src/types/apiKey.ts`** - TypeScript type definitions for API keys

## Features

### ✅ Implemented Features

1. **API Key List View**
   - Display all user's API keys in a table format
   - Show masked keys for security
   - Display key status (Active, Expired, Revoked)
   - Show last used date and creation date
   - Responsive design for mobile devices

2. **Create New API Key**
   - Modal dialog for creating new API keys
   - Name input with validation
   - Optional expiration date selection
   - Display newly created key with copy functionality
   - Security warning to copy key immediately

3. **Key Management Actions**
   - Copy key to clipboard functionality
   - Revoke/delete keys with confirmation dialog
   - Show/hide key visibility toggle for newly created keys

4. **Security Features**
   - Keys are masked in the UI after creation
   - One-time display of full key on creation
   - Confirmation dialogs for destructive actions
   - Input validation and error handling

5. **User Experience**
   - Toast notifications for user feedback
   - Loading states for async operations
   - Error handling with user-friendly messages
   - Consistent design with existing UI components

### 🎯 Key Security Considerations

1. **Key Display**: Full API keys are only shown once upon creation
2. **Masking**: Stored keys are always displayed in masked format
3. **Copy Protection**: Keys can be copied to clipboard but not selected/highlighted
4. **Confirmation**: Destructive actions require user confirmation
5. **Auto-dismiss**: New key display can be manually dismissed

### 🔧 Technical Implementation

- **Framework**: React with TypeScript
- **UI Components**: Radix UI + shadcn/ui components
- **Styling**: Tailwind CSS
- **State Management**: React hooks (useState, useEffect)
- **HTTP Client**: Axios
- **Notifications**: Sonner toast library
- **Icons**: Lucide React

### 📡 API Integration

The frontend integrates with the following API endpoints:

- `POST /api/v1/api-keys` - Create new API key
- `GET /api/v1/api-keys` - List user's API keys
- `GET /api/v1/api-keys/{id}` - Get specific API key details
- `PUT /api/v1/api-keys/{id}` - Update API key metadata
- `DELETE /api/v1/api-keys/{id}` - Revoke API key

### 🚀 Usage

1. Navigate to the API Keys section in the Developer Console sidebar
2. Click "Create API Key" to generate a new key
3. Enter a descriptive name and optional expiration date
4. Copy the generated key immediately (it won't be shown again)
5. Manage existing keys from the main table view

### 🎨 UI Components Used

- `Button` - Actions and form submissions
- `Input` - Text and date inputs
- `Dialog` - Create key modal
- `AlertDialog` - Delete confirmation
- `Table` - Key listing
- `Badge` - Status indicators
- `Toast` - User notifications

### 🔄 Future Enhancements

Potential improvements that could be added:
- Key usage analytics and charts
- Bulk key operations
- Key rotation functionality
- Advanced filtering and search
- Export key list to CSV
- Key usage history details
